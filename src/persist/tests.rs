//! Unit tests for WAL framing, checksums, sparse/dictionary codecs, locks, and snapshots.

use serde::Serialize;
use super::codec::*;
use super::snapshot::*;
use super::types::*;
use super::wal::*;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use crate::error::Error;
use crate::store::{Edge, Row};

#[cfg(all(test, unix))]
mod writer_lock_inheritance_tests {
    use super::*;

    #[test]
    fn fork_child_cannot_unlock_parent_and_cannot_extend_closed_writer_lock() {
        let dir = std::env::temp_dir().join(format!(
            "lin-lock-inheritance-unit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (_, persist) =
            crate::Store::open_with(&dir, &crate::catalog::fixture(), OpenMemOpts::default())
                .unwrap();
        let mut ready = [0; 2];
        let mut finish = [0; 2];
        assert_eq!(unsafe { libc::pipe(ready.as_mut_ptr()) }, 0);
        assert_eq!(unsafe { libc::pipe(finish.as_mut_ptr()) }, 0);
        let child = unsafe { libc::fork() };
        assert!(child >= 0);
        if child == 0 {
            // Exercise the exact Drop unlock path without dropping heap objects
            // or running the engine in a forked multithreaded test process.
            persist.release_writer_lock();
            unsafe {
                libc::close(ready[0]);
                libc::close(finish[1]);
                let mut byte = 1u8;
                libc::write(ready[1], (&byte as *const u8).cast(), 1);
                libc::read(finish[0], (&mut byte as *mut u8).cast(), 1);
                libc::_exit(0);
            }
        }
        unsafe {
            libc::close(ready[1]);
            libc::close(finish[0]);
        }
        let mut byte = 0u8;
        let received = unsafe { libc::read(ready[0], (&mut byte as *mut u8).cast(), 1) };
        let parent_still_locked = acquire_writer_lock(&dir).is_err();
        drop(persist);
        let reopened = acquire_writer_lock(&dir);
        unsafe {
            libc::write(finish[1], (&byte as *const u8).cast(), 1);
            libc::close(finish[1]);
            libc::close(ready[0]);
        }
        let mut status = 0;
        let reaped = unsafe { libc::waitpid(child, &mut status, 0) };
        let reopened_ok = reopened.is_ok();
        drop(reopened);
        fs::remove_dir_all(dir).unwrap();
        assert_eq!(received, 1);
        assert_eq!(reaped, child);
        assert_eq!(status, 0);
        assert!(
            parent_still_locked,
            "child released its parent's live writer lock"
        );
        assert!(
            reopened_ok,
            "inherited descriptor kept closed writer locked"
        );
    }
}


#[cfg(test)]
mod wal_integrity_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn parallel_sparse_rows_preserve_serial_bytes_and_worker_fallback() {
        let rows = (0..4101)
            .map(|i| match i % 13 {
                0 => None,
                1 => Some(Vec::new()),
                _ => {
                    let mut values = vec![0.0; 768];
                    values[7] = -0.0;
                    values[8] = f32::from_bits(0x7fc00001);
                    values[32] = f32::INFINITY;
                    values[767] = i as f32;
                    Some(values)
                }
            })
            .collect::<Vec<_>>();
        let mut expected = vec![1, 2, 3];
        // Independent scalar reference, without block skipping or workers.
        for row in &rows {
            match row {
                None => expected.extend_from_slice(&0u32.to_le_bytes()),
                Some(values) => {
                    expected.extend_from_slice(&((values.len() as u32) + 1).to_le_bytes());
                    let count = values.iter().filter(|x| x.to_bits() != 0).count() as u32;
                    expected.extend_from_slice(&count.to_le_bytes());
                    for (i, x) in values.iter().enumerate().filter(|(_, x)| x.to_bits() != 0) {
                        expected.extend_from_slice(&(i as u32).to_le_bytes());
                        expected.extend_from_slice(&x.to_bits().to_le_bytes());
                    }
                }
            }
        }
        let mut actual = vec![1, 2, 3];
        let shared_rows = rows
            .iter()
            .map(|v| v.as_ref().map(|v| std::sync::Arc::from(v.as_slice())))
            .collect::<Vec<_>>();
        append_sparse_rows_parallel(&shared_rows, &mut actual).unwrap();
        assert_eq!(actual, expected);
        let mut fallback = vec![1, 2, 3];
        append_sparse_worker(
            &mut fallback,
            &shared_rows,
            Err(io::ErrorKind::WouldBlock.into()),
        )
        .unwrap();
        assert_eq!(fallback, expected);
        with_log(|file| {
            let bytes = frame(file, vector_pack(rows));
            assert_eq!(bytes[8], 3);
            assert!(bytes[9..bytes.len() - 4].ends_with(&expected[3..]));
            let decoded = decode_column_record(&bytes[9..bytes.len() - 4], true).unwrap();
            let Pack::InsertCols { cols, n, .. } = decoded.pack else {
                panic!("columns");
            };
            assert_eq!(n, 4101);
            let ColData::Vec(v) = &cols[0] else {
                panic!("vectors");
            };
            assert!(v[0].is_none());
            assert!(v[1].as_ref().unwrap().is_empty());
            assert_eq!(v[4100].as_ref().unwrap()[767], 4100.0);
            assert_eq!(v[2].as_ref().unwrap()[7].to_bits(), (-0.0f32).to_bits());
            assert_eq!(v[2].as_ref().unwrap()[8].to_bits(), 0x7fc00001);
        });
    }

    #[test]
    fn parallel_sparse_columns_keep_shared_decoded_budget_before_writing() {
        let n = 4096;
        // Each column fits separately; together the vectors and row metadata
        // exceed the shared 64MiB decoded budget.
        let vectors: Vec<Option<std::sync::Arc<[f32]>>> = vec![Some(vec![0.0; 2048].into()); n];
        let record = LogRecord {
            r#gen: 1,
            next_id: 2,
            pack: Pack::InsertCols {
                collection: "test".into(),
                fields: vec!["a".into(), "b".into()],
                cols: vec![ColData::Vec(vectors.clone()), ColData::Vec(vectors)],
                n: n as u32,
                edges: vec![],
            },
        };
        with_log(|file| {
            file.write_all(b"previous").unwrap();
            let mut log_bytes = 8;
            let err = append_record(
                file,
                &record,
                &mut Vec::new(),
                SyncMode::Normal,
                &mut log_bytes,
            )
            .unwrap_err();
            assert!(err.message.contains("decoded 64MiB budget"));
            assert_eq!(log_bytes, 8);
            file.seek(SeekFrom::Start(0)).unwrap();
            let mut actual = Vec::new();
            file.read_to_end(&mut actual).unwrap();
            assert_eq!(actual, b"previous");
        });
    }

    #[test]
    fn parallel_sparse_classifier_preserves_exact_counts_and_selection() {
        for n in [0, 1, 3, 4095, 4096, 4101] {
            let rows = (0..n)
                .map(|i| {
                    if i % 11 == 0 {
                        None
                    } else {
                        let mut values = vec![0.0; 768];
                        values[7] = -0.0;
                        values[8] = f32::from_bits(0x7fc00001);
                        values[767] = f32::INFINITY;
                        Some(values.into())
                    }
                })
                .collect::<Vec<_>>();
            let expected = rows
                .iter()
                .map(|r| if r.is_some() { 8 + 3 * 8 } else { 4 })
                .sum::<usize>();
            assert_eq!(sparse_bytes_parallel(&rows), expected);
            assert_eq!(
                sparse_count_worker(&rows, Err(io::ErrorKind::WouldBlock.into())),
                expected
            );
            assert_eq!(use_sparse_vectors(&rows), n >= 4095);
        }
        let dense: Vec<Option<std::sync::Arc<[f32]>>> = vec![Some(vec![1.0; 768].into()); 4096];
        assert_eq!(sparse_bytes_parallel(&dense), 4096 * (8 + 768 * 8));
        assert!(!use_sparse_vectors(&dense));
        let mut empty = dense;
        empty[4095] = Some(std::sync::Arc::from([]));
        assert!(use_sparse_vectors(&empty));
    }

    #[test]
    fn shared_text_columns_preserve_legacy_bytes_and_row_values() {
        use crate::store::{Cell, Row};
        use std::sync::Arc;
        #[derive(Serialize)]
        #[serde(tag = "t", content = "v")]
        enum LegacyColumn {
            Text(Vec<String>),
        }
        let values = vec!["".to_owned(), "привет\0世界".to_owned()];
        let legacy = rmp_serde::to_vec_named(&LegacyColumn::Text(values.clone())).unwrap();
        let shared = ColData::Text(values.into_iter().map(Into::into).collect());
        assert_eq!(rmp_serde::to_vec_named(&shared).unwrap(), legacy);
        let decoded: ColData = rmp_serde::from_slice(&legacy).unwrap();
        assert_eq!(rmp_serde::to_vec_named(&decoded).unwrap(), legacy);
        let text: Arc<str> = Arc::from("привет\0世界");
        let rows = [Row::from([(
            "title".into(),
            Cell::text_arc(Arc::clone(&text)),
        )])];
        let Pack::InsertCols {
            fields, cols, n, ..
        } = rows_to_insert_cols("test", &rows, vec![])
        else {
            panic!("column pack")
        };
        let ColData::Text(column) = &cols[0] else {
            panic!("text column")
        };
        assert!(Arc::ptr_eq(&column[0], &text));
        let restored = cols_to_rows(&fields, &cols, n as usize);
        assert_eq!(restored[0], rows[0]);
        let Cell::Text(actual) = &restored[0]["title"] else {
            panic!("text cell")
        };
        assert!(Arc::ptr_eq(actual, &text));
    }

    #[test]
    fn row_major_column_pack_keeps_type_coercions_and_exact_wire_bytes() {
        use crate::store::{Cell, Row};
        let values = vec![-0.0f32, f32::from_bits(0x7fc00001), f32::INFINITY];
        let rows = [
            Row::from([
                ("bool".into(), Cell::Bool(true)),
                ("float".into(), Cell::Float(1.5)),
                ("int".into(), Cell::Int(10)),
                ("text".into(), Cell::text_arc("a")),
                ("time".into(), Cell::Time(12)),
                ("vector".into(), Cell::vec_arc(values.clone())),
            ]),
            Row::from([
                ("bool".into(), Cell::Null),
                ("float".into(), Cell::Int(7)),
                ("int".into(), Cell::Float(8.5)),
                ("text".into(), Cell::Int(99)),
                ("time".into(), Cell::Int(42)),
                ("vector".into(), Cell::Null),
            ]),
        ];
        let rows = rows.into_iter().cycle().take(4096).collect::<Vec<_>>();
        let actual = rows_to_insert_cols("test", &rows, vec![]);
        let expected = Pack::InsertCols {
            collection: "test".into(),
            fields: ["bool", "float", "int", "text", "time", "vector"]
                .map(String::from)
                .to_vec(),
            n: 4096,
            edges: vec![],
            cols: vec![
                ColData::Bool([true, false].into_iter().cycle().take(4096).collect()),
                ColData::Float([1.5, 7.0].into_iter().cycle().take(4096).collect()),
                ColData::Int([10, 0].into_iter().cycle().take(4096).collect()),
                ColData::Text(
                    [std::sync::Arc::from("a"), std::sync::Arc::from("")]
                        .into_iter()
                        .cycle()
                        .take(4096)
                        .collect(),
                ),
                ColData::Time([12, 42].into_iter().cycle().take(4096).collect()),
                ColData::Vec(
                    [Some(std::sync::Arc::<[f32]>::from(values)), None]
                        .into_iter()
                        .cycle()
                        .take(4096)
                        .collect(),
                ),
            ],
        };
        with_log(|file| {
            let actual = frame(file, actual);
            file.set_len(0).unwrap();
            file.seek(SeekFrom::Start(0)).unwrap();
            let expected = frame(file, expected);
            assert_eq!(actual, expected);
        });
        let rows = [
            Row::from([("x".into(), Cell::Null)]),
            Row::from([("x".into(), Cell::Int(9))]),
        ];
        let Pack::InsertCols { cols, .. } = rows_to_insert_cols("test", &rows, vec![]) else {
            panic!("columns")
        };
        assert!(matches!(&cols[0],ColData::Int(values) if values==&[0,9]));
    }

    #[test]
    fn column_field_union_keeps_later_extra_and_replaced_fields() {
        use crate::store::{Cell, Row};
        let rows = [
            Row::from([("a".into(), Cell::Int(1)), ("b".into(), Cell::Int(2))]),
            Row::from([("a".into(), Cell::Int(3)), ("b".into(), Cell::Int(4))]),
            Row::from([("a".into(), Cell::Int(5)), ("c".into(), Cell::Int(6))]),
            Row::from([("d".into(), Cell::Int(7))]),
        ];
        let Pack::InsertCols {
            fields, cols, n, ..
        } = rows_to_insert_cols("test", &rows, vec![])
        else {
            panic!("column pack")
        };
        assert_eq!(fields, ["a", "b", "c", "d"]);
        assert_eq!(n, 4);
        let actual = cols_to_rows(&fields, &cols, n as usize);
        for (original, decoded) in rows.iter().zip(actual) {
            for field in &fields {
                assert_eq!(
                    decoded[field],
                    original.get(field).cloned().unwrap_or(Cell::Int(0))
                );
            }
        }
    }

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn with_log(test: impl FnOnce(&mut File)) {
        let path = std::env::temp_dir().join(format!(
            "lin-wal-integrity-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        test(&mut file);
        drop(file);
        fs::remove_file(path).unwrap();
    }

    fn frame(file: &mut File, pack: Pack) -> Vec<u8> {
        append_record(
            file,
            &LogRecord {
                r#gen: 1,
                next_id: 2,
                pack,
            },
            &mut Vec::new(),
            SyncMode::Normal,
            &mut 0,
        )
        .unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        bytes
    }

    fn vector_pack(vectors: Vec<Option<Vec<f32>>>) -> Pack {
        Pack::InsertCols {
            collection: "docs".into(),
            fields: vec!["embedding".into()],
            n: vectors.len() as u32,
            cols: vec![ColData::Vec(
                vectors.into_iter().map(|v| v.map(Into::into)).collect(),
            )],
            edges: Vec::new(),
        }
    }

    #[test]
    fn sparse_vectors_preserve_bits_presence_and_checksum() {
        let expected = vec![
            None,
            Some(Vec::new()),
            Some(vec![
                0.0,
                -0.0,
                f32::from_bits(0x7fc00001),
                f32::INFINITY,
                -1.25,
            ]),
            Some({
                let mut values = vec![0.0; 69];
                values[7] = -0.0;
                values[8] = f32::from_bits(0x7fc00002);
                values[32] = f32::INFINITY;
                values[68] = -1.25;
                values
            }),
        ];
        with_log(|file| {
            let bytes = frame(file, vector_pack(expected.clone()));
            assert_eq!(bytes[8], 3);
            for_each_wal_frame(&bytes, |record| {
                let Pack::InsertCols { cols, .. } = record.pack else {
                    panic!("insert")
                };
                let ColData::Vec(actual) = &cols[0] else {
                    panic!("vectors")
                };
                let bits = |rows: &[Option<std::sync::Arc<[f32]>>]| {
                    rows.iter()
                        .map(|row| {
                            row.as_ref()
                                .map(|v| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>())
                        })
                        .collect::<Vec<_>>()
                };
                let expected_shared = expected
                    .iter()
                    .map(|v| v.clone().map(Into::into))
                    .collect::<Vec<_>>();
                assert_eq!(bits(actual), bits(&expected_shared));
                Ok(())
            })
            .unwrap();
            for offset in [8, 9, bytes.len() - 5, bytes.len() - 1] {
                let mut corrupt = bytes.clone();
                corrupt[offset] ^= 1;
                assert!(for_each_wal_frame(&corrupt, |_| Ok(())).is_err());
            }
            for cut in 1..bytes.len() {
                assert!(for_each_wal_frame(&bytes[..cut], |_| Ok(())).is_err());
            }
        });
        with_log(|file| {
            let bytes = frame(file, vector_pack(vec![Some(vec![1.0; 1024]); 100]));
            assert_eq!(bytes[8], 2, "dense data keeps legacy raw codec");
            assert_eq!(for_each_wal_frame(&bytes, |_| Ok(())).unwrap(), 1);
        });
    }

    #[test]
    fn sparse_vector_decoder_rejects_dimensions_indices_and_trailing_bytes() {
        with_log(|file| {
            let bytes = frame(
                file,
                vector_pack(vec![Some(vec![0.0, 1.0, 0.0, 2.0]), Some(Vec::new())]),
            );
            let raw = &bytes[9..bytes.len() - 4];
            assert!(
                decode_v2(raw).is_err(),
                "legacy codec must reject new vector tag"
            );
            let mut cursor = 17;
            take_str(raw, &mut cursor).unwrap();
            take_u32(raw, &mut cursor).unwrap();
            take_u32(raw, &mut cursor).unwrap();
            take_str(raw, &mut cursor).unwrap();
            assert_eq!(raw[cursor], 7);
            cursor += 1;
            for (offset, value) in [
                (cursor, u32::MAX),
                (cursor + 4, 5),
                (cursor + 8, 4),
                (cursor + 16, 1),
            ] {
                let mut bad = raw.to_vec();
                bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                assert!(decode_column_record(&bad, true).is_err());
            }
            let mut extra = raw.to_vec();
            extra.push(0);
            assert!(decode_column_record(&extra, true).is_err());
            let mut wrong_kind = raw.to_vec();
            wrong_kind[0] = V2_FACTS_BULK;
            assert!(decode_column_record(&wrong_kind, true).is_err());
        });
    }

    #[test]
    fn checksum_detects_bit_flips_in_both_codecs() {
        for pack in [
            Pack::Batch { packs: Vec::new() },
            Pack::AppendFactsBulk {
                s: vec![Arc::from("subject")],
                p: vec![Arc::from("predicate")],
                o: vec![Arc::from("object")],
            },
        ] {
            with_log(|file| {
                let bytes = frame(file, pack);
                assert_eq!(&bytes[..4], &LOG_MAGIC_CHECKSUM);
                assert_eq!(for_each_wal_frame(&bytes, |_| Ok(())).unwrap(), 1);
                for offset in 8..bytes.len() {
                    let mut corrupt = bytes.clone();
                    corrupt[offset] ^= 1;
                    assert!(
                        for_each_wal_frame(&corrupt, |_| Ok(())).is_err(),
                        "offset {offset}"
                    );
                }
                // Old v1/v2 envelopes remain readable by both shipping and replay.
                let mut legacy = vec![b'L', b'I', b'N', bytes[8]];
                let payload = &bytes[9..bytes.len() - 4];
                legacy.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                legacy.extend_from_slice(payload);
                assert_eq!(for_each_wal_frame(&legacy, |_| Ok(())).unwrap(), 1);
                file.set_len(0).unwrap();
                file.seek(SeekFrom::Start(0)).unwrap();
                file.write_all(&legacy).unwrap();
                let mut seen = 0;
                assert_eq!(
                    replay_log(file, 0, 0, |_| {
                        seen += 1;
                        Ok(())
                    })
                    .unwrap(),
                    legacy.len() as u64
                );
                assert_eq!(seen, 1);
            });
        }
    }

    #[test]
    fn every_truncated_checksummed_tail_is_ignored_but_complete_corruption_fails() {
        with_log(|file| {
            let bytes = frame(file, Pack::Batch { packs: Vec::new() });
            for length in 0..bytes.len() {
                file.set_len(0).unwrap();
                file.seek(SeekFrom::Start(0)).unwrap();
                file.write_all(&bytes[..length]).unwrap();
                assert_eq!(
                    replay_log(file, 0, 0, |_| panic!("partial record replayed")).unwrap(),
                    0
                );
                assert_eq!(file.metadata().unwrap().len(), length as u64);
            }
            let mut corrupt = bytes.clone();
            corrupt[9] ^= 1;
            corrupt.extend_from_slice(&bytes);
            file.set_len(0).unwrap();
            file.seek(SeekFrom::Start(0)).unwrap();
            file.write_all(&corrupt).unwrap();
            assert!(replay_log(file, 0, 0, |_| Ok(())).is_err());
            assert_eq!(file.metadata().unwrap().len(), corrupt.len() as u64);
        });
    }

    #[test]
    fn wal_hardware_crc32c_and_legacy_crc32_both_replay_cleanly() {
        with_log(|file| {
            let pack = Pack::AppendFactsBulk {
                s: vec![Arc::from("s")],
                p: vec![Arc::from("p")],
                o: vec![Arc::from("o")],
            };
            let hw_frame = frame(file, pack);
            assert_eq!(&hw_frame[..4], &LOG_MAGIC_CHECKSUM);

            // Verify the frame written uses hardware Castagnoli CRC32-C
            let end = hw_frame.len() - 4;
            let written_crc = u32::from_le_bytes(hw_frame[end..].try_into().unwrap());
            let expected_crc32c = crate::crc::hardware_crc32c(&hw_frame[8..end]);
            assert_eq!(written_crc, expected_crc32c);

            // Construct an equivalent legacy frame using IEEE 802.3 CRC32
            let mut legacy_frame = hw_frame.clone();
            let legacy_crc = crc32fast::hash(&hw_frame[8..end]);
            legacy_frame[end..].copy_from_slice(&legacy_crc.to_le_bytes());

            // Both frames must replay cleanly without errors
            assert_eq!(for_each_wal_frame(&hw_frame, |_| Ok(())).unwrap(), 1);
            assert_eq!(for_each_wal_frame(&legacy_frame, |_| Ok(())).unwrap(), 1);

            // Any corrupt CRC must fail
            let mut bad_frame = hw_frame.clone();
            bad_frame[end] ^= 0xff;
            assert!(for_each_wal_frame(&bad_frame, |_| Ok(())).is_err());
        });
    }
}

// Benchmark utilities for WAL operations
#[cfg(all(test, feature = "parallel"))]
mod bench {
    use super::*;
    use std::time::Instant;

    #[test]
    fn bench_wal_batch_write() {
        let path = std::env::temp_dir().join(format!("lin-bench-wal-batch-{}", std::process::id()));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .unwrap();

        let records = (0..1000)
            .map(|i| LogRecord {
                r#gen: i as u64 + 1,
                next_id: i as u64 + 1,
                pack: Pack::InsertBulk {
                    collection: "bench".into(),
                    rows: vec![crate::store::Row::new()],
                    edges: vec![],
                },
            })
            .collect::<Vec<_>>();

        let start = Instant::now();
        append_batch_records(&mut file, &records, SyncMode::Normal, &mut 0).unwrap();
        let elapsed = start.elapsed();

        let mut replayed = 0usize;
        replay_log(&mut file, 0, 0, |_| {
            replayed += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(replayed, 1000);
        drop(file);
        let _ = fs::remove_file(path);

        println!(
            "BATCH WRITE: {}ms for {} records",
            elapsed.as_millis(),
            records.len()
        );
        assert!(elapsed.as_millis() < 2000, "Batch write too slow");
    }
}

#[cfg(test)]
mod shared_vector_wire_tests {
    use super::*;
    #[test]
    fn shared_vector_columns_keep_legacy_messagepack_and_share_row_storage() {
        use crate::store::{Cell, Row};
        use std::sync::Arc;
        #[derive(Serialize)]
        #[serde(tag = "t", content = "v")]
        enum LegacyColumn {
            Vec(Vec<Option<Vec<f32>>>),
        }
        let values = vec![Some(vec![0.0, -0.0, 1.25]), None, Some(vec![])];
        let legacy = rmp_serde::to_vec_named(&LegacyColumn::Vec(values.clone())).unwrap();
        let shared = ColData::Vec(values.into_iter().map(|v| v.map(Into::into)).collect());
        assert_eq!(rmp_serde::to_vec_named(&shared).unwrap(), legacy);
        let decoded: ColData = rmp_serde::from_slice(&legacy).unwrap();
        assert_eq!(rmp_serde::to_vec_named(&decoded).unwrap(), legacy);
        let embedding: Arc<[f32]> = Arc::from([0.0, -0.0, 1.25]);
        let rows = [Row::from([(
            "embedding".into(),
            Cell::vec_arc(Arc::clone(&embedding)),
        )])];
        let Pack::InsertCols {
            fields, cols, n, ..
        } = rows_to_insert_cols("test", &rows, vec![])
        else {
            panic!("column pack")
        };
        let ColData::Vec(column) = &cols[0] else {
            panic!("vector column")
        };
        assert!(Arc::ptr_eq(column[0].as_ref().unwrap(), &embedding));
        let restored = cols_to_rows(&fields, &cols, n as usize);
        let Cell::Vec(restored_embedding) = &restored[0]["embedding"] else {
            panic!("vector cell")
        };
        assert!(Arc::ptr_eq(restored_embedding, &embedding));
    }
}

#[cfg(test)]
mod dictionary_wal_tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    fn fresh() -> PathBuf {
        std::env::temp_dir().join(format!(
            "lin-dictionary-reader-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ))
    }
    // Independent scalar fixture encoder: sorted dictionaries, unlike the CPU prototype's insertion order.
    fn rows_bytes(rows: &[Option<Vec<u32>>]) -> Vec<u8> {
        let mut out = Vec::new();
        for row in rows {
            let Some(values) = row else {
                out.extend_from_slice(&0u32.to_le_bytes());
                continue;
            };
            out.extend_from_slice(&u32::try_from(values.len() + 1).unwrap().to_le_bytes());
            let entries = values
                .iter()
                .enumerate()
                .filter(|(_, b)| **b != 0)
                .map(|(i, b)| (i, *b))
                .collect::<Vec<_>>();
            out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
            let dictionary = entries
                .iter()
                .map(|(_, b)| *b)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let use_dict = !dictionary.is_empty() && dictionary.len() <= 16;
            out.push(if use_dict { dictionary.len() as u8 } else { 0 });
            if use_dict {
                for bits in &dictionary {
                    out.extend_from_slice(&bits.to_le_bytes());
                }
            }
            for (index, bits) in entries {
                if use_dict && values.len() <= 65536 {
                    out.extend_from_slice(&(index as u16).to_le_bytes());
                } else {
                    out.extend_from_slice(&(index as u32).to_le_bytes());
                }
                if use_dict {
                    out.push(dictionary.binary_search(&bits).unwrap() as u8);
                } else {
                    out.extend_from_slice(&bits.to_le_bytes());
                }
            }
        }
        out
    }
    fn payload(n: u32, columns: Vec<(&str, u8, Vec<u8>)>) -> Vec<u8> {
        let mut out = vec![V2_INSERT_COLS];
        out.extend_from_slice(&2u64.to_le_bytes());
        out.extend_from_slice(&3u64.to_le_bytes());
        put_str(&mut out, "docs");
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&(columns.len() as u32).to_le_bytes());
        for (name, tag, bytes) in columns {
            put_str(&mut out, name);
            out.push(tag);
            out.extend(bytes);
        }
        out
    }
    fn dict_payload(row: Vec<u8>) -> Vec<u8> {
        payload(1, vec![("embedding", 8, row)])
    }
    fn bits(rec: LogRecord) -> Vec<Option<Vec<u32>>> {
        let Pack::InsertCols { cols, .. } = rec.pack else {
            panic!("insert")
        };
        let ColData::Vec(rows) = cols.into_iter().next().unwrap() else {
            panic!("vectors")
        };
        rows.into_iter()
            .map(|v| v.map(|v| v.iter().map(|x| x.to_bits()).collect()))
            .collect()
    }
    fn frame(payload: &[u8]) -> Vec<u8> {
        let path = fresh();
        let mut f = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        write_frame(
            &mut f,
            &LOG_MAGIC_DICTIONARY,
            payload,
            SyncMode::Normal,
            &mut 0,
        )
        .unwrap();
        f.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).unwrap();
        drop(f);
        fs::remove_file(path).unwrap();
        bytes
    }
    #[test]
    fn dictionary_materialization_keeps_empty_separate_from_null() {
        let columns = [ColData::Vec(vec![
            None,
            Some(Vec::<f32>::new().into()),
            Some(vec![-0.0].into()),
        ])];
        let rows = cols_to_rows(&["embedding".into()], &columns, 3);
        assert!(matches!(rows[0]["embedding"], crate::Cell::Null));
        assert_eq!(rows[1]["embedding"].as_vec(), Some(&[][..]));
        assert_eq!(
            rows[2]["embedding"].as_vec().unwrap()[0].to_bits(),
            0x80000000
        );
    }
    #[test]
    fn dictionary_mixed_vector_columns_share_budget_and_preserve_bits() {
        let a = rows_bytes(&[Some(vec![0x80000000, 0x7fc00001])]);
        let sparse = [Some(std::sync::Arc::<[f32]>::from([
            f32::from_bits(0x7fc00002),
            -0.0,
        ]))];
        let mut b = Vec::new();
        append_sparse_rows_serial(&sparse, &mut b).unwrap();
        let mut c = Vec::new();
        c.extend_from_slice(&2u32.to_le_bytes());
        c.extend_from_slice(&0x7fa00001u32.to_le_bytes());
        c.extend_from_slice(&0x80000000u32.to_le_bytes());
        let p = payload(1, vec![("a", 8, a), ("b", 7, b), ("c", 6, c)]);
        let rec = decode_column_record_mode(&p, true, true).unwrap();
        let Pack::InsertCols { cols, .. } = rec.pack else {
            panic!("insert")
        };
        let actual = cols
            .into_iter()
            .map(|c| {
                let ColData::Vec(v) = c else { panic!("vector") };
                v[0].as_ref()
                    .unwrap()
                    .iter()
                    .map(|x| x.to_bits())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                vec![0x80000000, 0x7fc00001],
                vec![0x7fc00002, 0x80000000],
                vec![0x7fa00001, 0x80000000]
            ]
        );
    }
    #[test]
    fn dictionary_reader_does_not_enable_new_writer_codec() {
        let path = fresh();
        let mut f = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let mut v = vec![0.0; 768];
        v[7] = -0.0;
        let rec = LogRecord {
            r#gen: 1,
            next_id: 2,
            pack: Pack::InsertCols {
                collection: "docs".into(),
                fields: vec!["embedding".into()],
                cols: vec![ColData::Vec(vec![Some(std::sync::Arc::from(v)); 1000])],
                n: 1000,
                edges: vec![],
            },
        };
        append_record(&mut f, &rec, &mut Vec::new(), SyncMode::Normal, &mut 0).unwrap();
        f.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).unwrap();
        assert_eq!(&bytes[..4], &LOG_MAGIC_CHECKSUM);
        assert_eq!(bytes[8], 3);
        decode_frame(&LOG_MAGIC_CHECKSUM, &bytes[8..]).unwrap();
        drop(f);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn dictionary_roundtrip_bits_widths_none_empty_and_raw_escape() {
        let mut rows = vec![
            None,
            Some(vec![]),
            Some(vec![0; 9]),
            Some(vec![
                0x80000000, 0x7fc00001, 0x7fc00002, 0x7fa00001, 0x7f800000, 0xff800000, 0x80000000,
            ]),
        ];
        for dim in [8, 9, 65535, 65536, 65537] {
            let mut v = vec![0; dim];
            v[0] = 0x80000000;
            v[dim / 2] = 0x7fc00001;
            v[dim - 1] = 0x3fc00000;
            rows.push(Some(v));
        }
        rows.push(Some((1..=64).map(|i| 0x3f000000 + i).collect()));
        rows.push(Some((0..32).map(|i| 0x3f000000 + (i % 16) + 1).collect()));
        let p = payload(rows.len() as u32, vec![("embedding", 8, rows_bytes(&rows))]);
        assert_eq!(
            bits(decode_column_record_mode(&p, true, true).unwrap()),
            rows
        );
        let f = frame(&p);
        assert_eq!(&f[..4], &LOG_MAGIC_CHECKSUM);
        assert_eq!(f[8], 5);
        assert_eq!(
            bits(decode_frame(&LOG_MAGIC_CHECKSUM, &f[8..]).unwrap()),
            rows
        );
        let mut ieee = f.clone();
        let end = ieee.len() - 4;
        let crc = crc32fast::hash(&ieee[8..end]).to_le_bytes();
        ieee[end..].copy_from_slice(&crc);
        assert_eq!(
            bits(decode_frame(&LOG_MAGIC_CHECKSUM, &ieee[8..]).unwrap()),
            rows
        );

        assert!(decode_column_record(&p, true).is_err());
        assert!(decode_v2(&p).is_err());
        let mut bad = f.clone();
        bad[15] ^= 1;
        assert!(decode_frame(&LOG_MAGIC_CHECKSUM, &bad[8..]).is_err());
    }
    #[test]
    fn dictionary_rejects_bad_tables_indices_counts_ordinals_and_trailing_bytes() {
        let valid = rows_bytes(&[Some(vec![0x3f800000, 0, 0, 0])]); // d=1, index u16, ordinal at 15.
        assert_eq!(valid.len(), 16);
        let mut cases = Vec::new();
        let mut b = valid.clone();
        b[8] = 17;
        cases.push(b);
        let mut b = valid.clone();
        b[9..13].fill(0);
        cases.push(b);
        let mut b = valid.clone();
        b[13..15].copy_from_slice(&4u16.to_le_bytes());
        cases.push(b);
        let mut b = valid.clone();
        b[15] = 1;
        cases.push(b);
        let mut b = valid.clone();
        b[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        cases.push(b);
        let mut b = valid.clone();
        b[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
        cases.push(b);
        let mut b = valid.clone();
        b.push(0);
        cases.push(b);
        let two = rows_bytes(&[Some(vec![0x3f800000, 0x40000000, 0, 0])]);
        let mut b = two.clone();
        b[13..17].copy_from_slice(&0x3f800000u32.to_le_bytes());
        cases.push(b); // duplicate dictionary value.
        let mut b = two.clone();
        b[22] = 0;
        cases.push(b); // unused second dictionary entry.
        let mut b = two.clone();
        b[20..22].copy_from_slice(&0u16.to_le_bytes());
        cases.push(b); // duplicate index.
        let mut b = two.clone();
        b[17..19].copy_from_slice(&2u16.to_le_bytes());
        cases.push(b); // descending indexes.
        let mut raw = Vec::new();
        raw.extend_from_slice(&5u32.to_le_bytes());
        raw.extend_from_slice(&1u32.to_le_bytes());
        raw.push(0);
        raw.extend_from_slice(&0u32.to_le_bytes());
        raw.extend_from_slice(&0u32.to_le_bytes());
        cases.push(raw); // redundant +0 raw entry.
        for b in cases {
            let payload = dict_payload(b);
            assert!(decode_column_record_mode(&payload, true, true).is_err());
            let bytes = frame(&payload);
            assert!(decode_frame(&LOG_MAGIC_CHECKSUM, &bytes[8..]).is_err());
        }
        let p = dict_payload(valid);
        for end in 0..p.len() {
            assert!(
                decode_column_record_mode(&p[..end], true, true).is_err(),
                "prefix {end}"
            );
        }
        for tag in [0, 9, 255] {
            let mut b = payload(1, vec![("embedding", tag, vec![])]);
            assert!(decode_column_record_mode(&b, true, true).is_err());
            b.push(0);
        }
        let no_dictionary = payload(1, vec![("embedding", 7, vec![0; 4])]);
        assert!(decode_column_record_mode(&no_dictionary, true, true).is_err());
    }
    #[test]
    fn dictionary_budget_covers_metadata_vectors_and_mixed_dense_columns() {
        let b = rows_bytes(&[Some(vec![0x3f800000])]);
        let mut offset = 0;
        let mut budget = MAX_SPARSE_DECODED_BYTES - std::mem::size_of::<Option<Vec<f32>>>();
        assert!(take_dictionary_vectors(&b, &mut offset, 1, &mut budget).is_err()); // dimension rejected before value allocation.
        let mut offset = 0;
        let mut budget = 0;
        assert!(
            take_dictionary_vectors(
                &[],
                &mut offset,
                MAX_SPARSE_DECODED_BYTES / 24 + 1,
                &mut budget
            )
            .is_err()
        );
        let too_many = payload(
            (MAX_SPARSE_DECODED_BYTES / 24 + 1) as u32,
            vec![("embedding", 8, vec![])],
        );
        assert!(decode_column_record_mode(&too_many, true, true).is_err());
        // Two zero-valued dictionary vectors: 32MiB each plus metadata exceeds shared 64MiB.
        let mut row = Vec::new();
        row.extend_from_slice(&(8_388_608u32 + 1).to_le_bytes());
        row.extend_from_slice(&0u32.to_le_bytes());
        row.push(0);
        let p = payload(1, vec![("a", 8, row.clone()), ("b", 8, row)]);
        assert!(decode_column_record_mode(&p, true, true).is_err());
        // Include legacy dense allocation in codec-5 shared accounting.
        let mut row = Vec::new();
        row.extend_from_slice(&(16_777_210u32 + 1).to_le_bytes());
        row.extend_from_slice(&0u32.to_le_bytes());
        row.push(0);
        let mut dense = Vec::new();
        dense.extend_from_slice(&1u32.to_le_bytes());
        dense.extend_from_slice(&1f32.to_le_bytes());
        assert!(
            decode_column_record_mode(
                &payload(1, vec![("a", 8, row), ("b", 6, dense)]),
                true,
                true
            )
            .is_err()
        );
    }
    #[test]
    fn dictionary_torn_tails_and_complete_corruption_follow_replay_contract() {
        let p = dict_payload(rows_bytes(&[Some(vec![0x80000000, 0x3f800000])]));
        let valid = frame(&p);
        let path = fresh();
        let mut file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        for end in 0..valid.len() {
            file.set_len(0).unwrap();
            file.seek(SeekFrom::Start(0)).unwrap();
            file.write_all(&valid[..end]).unwrap();
            assert_eq!(
                replay_log(&mut file, 0, 0, |_| panic!("truncated frame applied")).unwrap(),
                0
            );
            assert_eq!(file.metadata().unwrap().len(), end as u64);
        }
        file.set_len(0).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&valid).unwrap();
        let mut applied = 0;
        assert_eq!(
            replay_log(&mut file, 0, 0, |_| {
                applied += 1;
                Ok(())
            })
            .unwrap(),
            valid.len() as u64
        );
        assert_eq!(applied, 1);
        let mut invalid = valid.clone();
        invalid[12] ^= 1;
        let mut invalid_payload = p.clone();
        *invalid_payload.last_mut().unwrap() = 255;
        let checked_but_invalid = frame(&invalid_payload);
        for bad in [invalid, checked_but_invalid] {
            let mut bytes = valid.clone();
            bytes.extend_from_slice(&bad);
            file.set_len(0).unwrap();
            file.seek(SeekFrom::Start(0)).unwrap();
            file.write_all(&bytes).unwrap();
            assert!(replay_log(&mut file, 0, 0, |_| Ok(())).is_err());
            file.seek(SeekFrom::Start(0)).unwrap();
            let mut after = Vec::new();
            file.read_to_end(&mut after).unwrap();
            assert_eq!(after, bytes);
        }
        drop(file);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn dictionary_mutated_payloads_never_panic() {
        let p = dict_payload(rows_bytes(&[Some(vec![
            0x80000000, 0x7fc00001, 0, 0x3f800000,
        ])]));
        for i in 0..p.len() {
            for bit in 0..8 {
                let mut b = p.clone();
                b[i] ^= 1 << bit;
                assert!(
                    std::panic::catch_unwind(|| decode_column_record_mode(&b, true, true)).is_ok()
                );
            }
        }
    }
    fn directory_bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    out.insert(
                        path.strip_prefix(root).unwrap().to_path_buf(),
                        fs::read(path).unwrap(),
                    );
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(root, root, &mut out);
        out
    }
    #[test]
    fn dictionary_legacy_sparse_empty_vector_survives_reopen_and_checkpoint() {
        use crate::{Db, OpenOpts};
        let root = fresh();
        assert!(!root.exists());
        {
            let mut db = Db::open_with(&root, OpenOpts::default()).unwrap();
            db.run("index docs [wing]").unwrap();
        }
        let pack = Pack::InsertCols {
            collection: "docs".into(),
            fields: vec!["id".into(), "uri".into(), "embedding".into()],
            cols: vec![
                ColData::Text(vec![std::sync::Arc::from("legacy-empty")]),
                ColData::Text(vec![std::sync::Arc::from("bench://legacy-empty")]),
                ColData::Vec(vec![Some(Vec::<f32>::new().into())]),
            ],
            n: 1,
            edges: vec![],
        };
        let mut file = open_log(&root).unwrap();
        file.seek(SeekFrom::End(0)).unwrap();
        let start = file.stream_position().unwrap();
        let mut len = start;
        append_record(
            &mut file,
            &LogRecord {
                r#gen: 2,
                next_id: 3,
                pack,
            },
            &mut Vec::new(),
            SyncMode::Full,
            &mut len,
        )
        .unwrap();
        file.seek(SeekFrom::Start(start + 8)).unwrap();
        let mut codec = [0];
        file.read_exact(&mut codec).unwrap();
        assert_eq!(codec[0], 3);
        drop(file);
        {
            let mut db = Db::open_with(&root, OpenOpts::default()).unwrap();
            assert_eq!(
                db.store.collection("docs")[0]["embedding"].as_vec(),
                Some(&[][..])
            );
            db.checkpoint().unwrap();
        }
        {
            let db = Db::open_with(&root, OpenOpts::default()).unwrap();
            assert_eq!(
                db.store.collection("docs")[0]["embedding"].as_vec(),
                Some(&[][..])
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn dictionary_reopen_fixture_and_old_reader_rejection() {
        use crate::{Db, OpenOpts};
        let preserve = std::env::var_os("LIN_DICTIONARY_FIXTURE").map(PathBuf::from);
        let root = preserve.clone().unwrap_or_else(fresh);
        assert!(!root.exists(), "refuse existing fixture");
        {
            let mut db = Db::open_with(&root, OpenOpts::default()).unwrap();
            db.run("index docs [wing]").unwrap();
            assert_eq!(db.r#gen(), 1);
        }
        let old = std::env::var_os("LIN_DICTIONARY_OLD_READER");
        if let Some(binary) = &old {
            let r = std::process::Command::new(binary)
                .arg(&root)
                .output()
                .unwrap();
            assert!(
                r.status.success(),
                "old reader must open pre-dictionary fixture: {:?}",
                r.stderr
            );
        }
        let expected = vec![
            Some(vec![
                0x80000000, 0x7fc00001, 0x7fa00001, 0x7f800000, 0xff800000, 0, 0x3f800000,
                0x80000000,
            ]),
            Some(vec![]),
        ];
        let mut ids = Vec::new();
        for id in ["dict-0", "dict-1"] {
            put_str(&mut ids, id);
        }
        let mut uris = Vec::new();
        for uri in ["bench://dict-0", "bench://dict-1"] {
            put_str(&mut uris, uri);
        }
        let p = payload(
            2,
            vec![
                ("id", 1, ids),
                ("uri", 1, uris),
                ("embedding", 8, rows_bytes(&expected)),
            ],
        );
        let mut file = open_log(&root).unwrap();
        file.seek(SeekFrom::End(0)).unwrap();
        let mut len = file.metadata().unwrap().len();
        write_frame(
            &mut file,
            &LOG_MAGIC_DICTIONARY,
            &p,
            SyncMode::Full,
            &mut len,
        )
        .unwrap();
        drop(file);
        if let Some(binary) = &old {
            let before = directory_bytes(&root);
            let r = std::process::Command::new(binary)
                .arg(&root)
                .output()
                .unwrap();
            assert_eq!(r.status.code(), Some(2));
            assert_eq!(
                directory_bytes(&root),
                before,
                "old reader must not change bytes"
            );
            println!(
                "DICT_COMPAT {}",
                serde_json::json!({"old_reader_exit":2,"files_unchanged":true,"files":before.iter().map(|(p,b)|(p.display().to_string(),b.len())).collect::<BTreeMap<_,_>>(),"stderr":String::from_utf8_lossy(&r.stderr)})
            );
        }
        let assert_rows = |db: &Db| {
            let actual = db
                .store
                .collection("docs")
                .iter()
                .map(|r| {
                    r.get("embedding")
                        .and_then(|x| x.as_vec())
                        .map(|v| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>())
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
        };
        let before_read = directory_bytes(&root);
        {
            let read = Db::open_read(&root).unwrap();
            let handle = read.run("docs | { embedding } | take all").unwrap();
            let actual = handle
                .rows
                .iter()
                .map(|row| {
                    row.get("embedding")
                        .and_then(|cell| cell.as_vec())
                        .map(|v| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>())
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
        }
        assert_eq!(
            directory_bytes(&root),
            before_read,
            "read-only replay must not mutate fixture"
        );
        if preserve.is_none() {
            {
                let mut db = Db::open_with(&root, OpenOpts::default()).unwrap();
                assert_rows(&db);
                db.checkpoint().unwrap();
            }
            {
                let db = Db::open_with(&root, OpenOpts::default()).unwrap();
                assert_rows(&db);
            }
            fs::remove_dir_all(root).unwrap();
        }
    }
}

#[cfg(test)]
mod wal_preallocation_tests {
    use super::*;
    use std::io::{Seek, SeekFrom};
    #[test]
    fn reservation_preserves_contents_length_position_and_read_only_open() {
        let path = std::env::temp_dir().join(format!(
            "lin-preallocation-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        let mut file = open_log(&path).unwrap();
        assert_eq!(file.metadata().unwrap().len(), 0);
        assert_eq!(file.stream_position().unwrap(), 0);
        file.write_all(b"prefix").unwrap();
        file.seek(SeekFrom::Start(2)).unwrap();
        reserve_wal_space(&file, 2 * 1024 * 1024, false);
        assert_eq!(file.metadata().unwrap().len(), 6);
        assert_eq!(file.stream_position().unwrap(), 2);
        assert_eq!(std::fs::read(log_path(&path)).unwrap(), b"prefix");
        reserve_wal_space(&file, usize::MAX, false);
        assert_eq!(file.metadata().unwrap().len(), 6);
        file.seek(SeekFrom::End(0)).unwrap();
        file.write_all(b"suffix").unwrap();
        drop(file);
        let file = open_log(&path).unwrap();
        assert_eq!(file.metadata().unwrap().len(), 12);
        drop(file);
        let before = std::fs::read(log_path(&path)).unwrap();
        drop(open_log_read(&path).unwrap());
        assert_eq!(std::fs::read(log_path(&path)).unwrap(), before);
        assert_eq!(before, b"prefixsuffix");
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[cfg(test)]
fn append_insert_cols_v2_reference(
    log: &mut File,
    rec_gen: u64,
    next_id: u64,
    collection: &str,
    fields: &[String],
    cols: &[ColData],
    n: u32,
    edges: &[Edge],
    buf: &mut Vec<u8>,
    sync: SyncMode,
    log_bytes: &mut u64,
) -> Result<(), Error> {
    // Fallback to msgpack if edges present (rare in bulk benches) or column mismatch.
    if !edges.is_empty() || fields.len() != cols.len() {
        return append_record_v1(
            log,
            &LogRecord {
                r#gen: rec_gen,
                next_id,
                pack: Pack::InsertCols {
                    collection: collection.to_string(),
                    fields: fields.to_vec(),
                    cols: cols.to_vec(),
                    n,
                    edges: edges.to_vec(),
                },
            },
            buf,
            sync,
            log_bytes,
        );
    }
    let mut est_per_row = 32usize;
    for col in cols {
        match col {
            ColData::Vec(v) => {
                let dim = v
                    .first()
                    .and_then(|opt| opt.as_ref())
                    .map_or(0, |vec| vec.len());
                est_per_row = est_per_row.saturating_add(8 + dim * 4);
            }
            ColData::Text(_) => est_per_row = est_per_row.saturating_add(16),
            ColData::Int(_) | ColData::Float(_) | ColData::Time(_) => {
                est_per_row = est_per_row.saturating_add(8)
            }
            ColData::Bool(_) => est_per_row = est_per_row.saturating_add(1),
            ColData::Null => {}
        }
    }
    buf.clear();
    buf.reserve(64 + fields.len() * 16 + (n as usize) * est_per_row);
    buf.push(V2_INSERT_COLS);
    buf.extend_from_slice(&rec_gen.to_le_bytes());
    buf.extend_from_slice(&next_id.to_le_bytes());
    put_str(buf, collection);
    buf.extend_from_slice(&n.to_le_bytes());
    buf.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    let mut sparse_codec = false;
    let mut sparse_decoded_bytes = 0usize;
    for (f, c) in fields.iter().zip(cols.iter()) {
        put_str(buf, f);
        match c {
            ColData::Text(v) => {
                buf.push(1);
                for s in v {
                    put_str(buf, s);
                }
            }
            ColData::Int(v) => {
                buf.push(2);
                for x in v {
                    buf.extend_from_slice(&x.to_le_bytes());
                }
            }
            ColData::Float(v) => {
                buf.push(3);
                for x in v {
                    buf.extend_from_slice(&x.to_le_bytes());
                }
            }
            ColData::Bool(v) => {
                buf.push(4);
                for x in v {
                    buf.push(u8::from(*x));
                }
            }
            ColData::Time(v) => {
                buf.push(5);
                for x in v {
                    buf.extend_from_slice(&x.to_le_bytes());
                }
            }
            ColData::Vec(v) => {
                let mut dense = 0usize;
                let mut has_empty = false;
                for row in v {
                    match row.as_ref() {
                        None => dense = dense.saturating_add(4),
                        Some(emb) => {
                            if emb.is_empty() {
                                has_empty = true;
                            }
                            dense = dense.saturating_add(
                                4usize.saturating_add(emb.len().saturating_mul(4)),
                            );
                        }
                    }
                }
                let try_sparse = has_empty
                    || (dense >= SPARSE_VECTOR_THRESHOLD
                        && if v.len() >= 4096 {
                            use_sparse_vectors(v)
                        } else if let Some(Some(first)) = v.first()
                            && 8usize.saturating_add(count_non_zero_floats(first).saturating_mul(8))
                                > 4usize.saturating_add(first.len().saturating_mul(4))
                        {
                            use_sparse_vectors(v)
                        } else {
                            true
                        });
                let mut wrote_sparse = false;
                if try_sparse {
                    if v.len() >= 4096 {
                        if v.len() != n as usize {
                            return Err(io_err("sparse vector column length mismatch"));
                        }
                        let storage = v
                            .len()
                            .checked_mul(std::mem::size_of::<Option<Vec<f32>>>())
                            .ok_or_else(|| io_err("sparse vector size overflow"))?;
                        sparse_decoded_bytes = sparse_decoded_bytes
                            .checked_add(storage)
                            .ok_or_else(|| io_err("sparse vector size overflow"))?;
                        sparse_codec = true;
                        buf.push(7);
                        if v.iter()
                            .flatten()
                            .fold(0usize, |n, emb| n.saturating_add(emb.len()))
                            >= 2 * 1024 * 1024
                        {
                            for emb in v.iter().flatten() {
                                let bytes = emb
                                    .len()
                                    .checked_mul(4)
                                    .ok_or_else(|| io_err("sparse vector size overflow"))?;
                                sparse_decoded_bytes = sparse_decoded_bytes
                                    .checked_add(bytes)
                                    .filter(|&bytes| bytes <= MAX_SPARSE_DECODED_BYTES)
                                    .ok_or_else(|| {
                                        io_err("sparse vectors exceed decoded 64MiB budget")
                                    })?;
                            }
                            append_sparse_rows_parallel(v, buf)?;
                        } else {
                            for emb in v.iter().flatten() {
                                let bytes = emb
                                    .len()
                                    .checked_mul(4)
                                    .ok_or_else(|| io_err("sparse vector size overflow"))?;
                                sparse_decoded_bytes = sparse_decoded_bytes
                                    .checked_add(bytes)
                                    .filter(|&bytes| bytes <= MAX_SPARSE_DECODED_BYTES)
                                    .ok_or_else(|| {
                                        io_err("sparse vectors exceed decoded 64MiB budget")
                                    })?;
                            }
                            append_sparse_rows_serial(v, buf)?;
                        }
                        if sparse_decoded_bytes > MAX_SPARSE_DECODED_BYTES {
                            return Err(io_err("sparse vectors exceed decoded 64MiB budget"));
                        }
                        wrote_sparse = true;
                    } else {
                        let mark = buf.len();
                        buf.push(7);
                        let payload_start = buf.len();
                        if let Err(e) = append_sparse_rows_serial(v, buf) {
                            buf.truncate(mark);
                            return Err(e);
                        }
                        let sparse_bytes = buf.len() - payload_start;
                        if !has_empty && sparse_bytes >= dense {
                            buf.truncate(mark);
                        } else {
                            if v.len() != n as usize {
                                buf.truncate(mark);
                                return Err(io_err("sparse vector column length mismatch"));
                            }
                            let storage = v
                                .len()
                                .checked_mul(std::mem::size_of::<Option<Vec<f32>>>())
                                .ok_or_else(|| io_err("sparse vector size overflow"))?;
                            let mut next_decoded = sparse_decoded_bytes
                                .checked_add(storage)
                                .ok_or_else(|| io_err("sparse vector size overflow"))?;
                            for emb in v.iter().flatten() {
                                let bytes = emb
                                    .len()
                                    .checked_mul(4)
                                    .ok_or_else(|| io_err("sparse vector size overflow"))?;
                                next_decoded = next_decoded
                                    .checked_add(bytes)
                                    .filter(|&bytes| bytes <= MAX_SPARSE_DECODED_BYTES)
                                    .ok_or_else(|| {
                                        io_err("sparse vectors exceed decoded 64MiB budget")
                                    })?;
                            }
                            if next_decoded > MAX_SPARSE_DECODED_BYTES {
                                return Err(io_err("sparse vectors exceed decoded 64MiB budget"));
                            }
                            sparse_decoded_bytes = next_decoded;
                            sparse_codec = true;
                            wrote_sparse = true;
                        }
                    }
                }
                if !wrote_sparse {
                    buf.push(6);
                    for row in v {
                        match row {
                            Some(emb) if !emb.is_empty() => {
                                buf.extend_from_slice(&(emb.len() as u32).to_le_bytes());
                                #[cfg(target_endian = "little")]
                                {
                                    // Safety: [f32] on little-endian has the exact IEEE 754 LE byte representation.
                                    let bytes = unsafe {
                                        std::slice::from_raw_parts(
                                            emb.as_ptr() as *const u8,
                                            emb.len() * std::mem::size_of::<f32>(),
                                        )
                                    };
                                    buf.extend_from_slice(bytes);
                                }
                                #[cfg(not(target_endian = "little"))]
                                {
                                    for x in emb.iter() {
                                        buf.extend_from_slice(&x.to_le_bytes());
                                    }
                                }
                            }
                            _ => buf.extend_from_slice(&0u32.to_le_bytes()),
                        }
                    }
                }
            }
            ColData::Null => buf.push(0),
        }
    }
    write_frame(
        log,
        if sparse_codec {
            &LOG_MAGIC_SPARSE
        } else {
            &LOG_MAGIC_V2
        },
        buf,
        sync,
        log_bytes,
    )
}

#[cfg(test)]
mod pure_column_encoder_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    fn framed(
        reference: bool,
        fields: &[String],
        cols: &[ColData],
        n: u32,
        edges: &[Edge],
    ) -> (Result<Vec<u8>, String>, u64, Vec<u8>) {
        let path = std::env::temp_dir().join(format!(
            "lin-pure-column-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let mut buf = vec![1, 2, 3];
        let mut log_bytes = 0;
        let result = if reference {
            append_insert_cols_v2_reference(
                &mut file,
                9,
                42,
                "docs",
                fields,
                cols,
                n,
                edges,
                &mut buf,
                SyncMode::Normal,
                &mut log_bytes,
            )
        } else {
            append_insert_cols_v2(
                &mut file,
                9,
                42,
                "docs",
                fields,
                cols,
                n,
                edges,
                &mut buf,
                SyncMode::Normal,
                &mut log_bytes,
            )
        };
        let disk = std::fs::read(&path).unwrap();
        drop(file);
        std::fs::remove_file(path).unwrap();
        (
            result.map(|_| disk.clone()).map_err(|e| e.to_string()),
            log_bytes,
            disk,
        )
    }
    fn compare(fields: &[String], cols: &[ColData], n: u32, edges: &[Edge]) {
        let old = framed(true, fields, cols, n, edges);
        let new = framed(false, fields, cols, n, edges);
        assert_eq!(old, new);
        if edges.is_empty() && fields.len() == cols.len() {
            let mut buf = vec![4, 5, 6];
            let pure = encode_insert_cols_v2(9, 42, "docs", fields, cols, n, &mut buf);
            match (&old.0, pure) {
                (Ok(frame), Ok(magic)) => {
                    assert_eq!(frame[8], magic[3]);
                    assert_eq!(&frame[9..frame.len() - 4], buf);
                }
                (Err(a), Err(b)) => {
                    assert_eq!(*a, b.to_string());
                    assert!(old.2.is_empty());
                    assert_eq!(old.1, 0);
                }
                _ => panic!("pure/writer result mismatch"),
            }
        }
    }
    #[test]
    fn pure_payload_matches_original_frames_and_fallbacks() {
        let fields = ["body", "int", "float", "bool", "time", "vector", "null"].map(str::to_owned);
        let cols = vec![
            ColData::Text(
                (0..6)
                    .map(|i| Arc::from(format!("row {i} ЁЖ 😀")))
                    .collect(),
            ),
            ColData::Int(vec![0, -1, i64::MIN, i64::MAX, 7, 8]),
            ColData::Float(vec![
                0.0,
                -0.0,
                f64::from_bits(0x7ff8000000000001),
                f64::INFINITY,
                -1.25,
                3.5,
            ]),
            ColData::Bool(vec![false, true, false, true, false, true]),
            ColData::Time(vec![0, 1, -1, i64::MIN, i64::MAX, 123]),
            ColData::Vec(vec![
                None,
                Some(Arc::from([])),
                Some(Arc::from([0.0; 35])),
                Some(Arc::from([-0.0, f32::from_bits(0x7fc00001), f32::INFINITY])),
                Some(Arc::from([1.0, 0.0, -1.0])),
                Some(Arc::from([0.0, 2.0])),
            ]),
            ColData::Null,
        ];
        compare(&fields, &cols, 6, &[]);
        compare(&[], &[], 0, &[]);
        compare(
            &["embedding".into()],
            &[ColData::Vec(vec![Some(Arc::from(vec![1.0; 768])); 64])],
            64,
            &[],
        );
        compare(
            &["embedding".into()],
            &[ColData::Vec(vec![Some(Arc::from(vec![0.0; 768])); 4097])],
            4097,
            &[],
        );
        compare(&fields[..1], &cols[..2], 6, &[]);
        compare(
            &fields,
            &cols,
            6,
            &[Edge {
                rel: "links".into(),
                from: "a".into(),
                to: "b".into(),
            }],
        );
        let mut buf = vec![7, 8, 9];
        assert!(
            encode_insert_cols_v2(9, 42, "docs", &fields[..1], &cols[..2], 6, &mut buf).is_err()
        );
        assert_eq!(buf, [7, 8, 9]);
    }
    #[test]
    fn pure_payload_keeps_sparse_budget_failure_before_file_write() {
        let rows = vec![Some(Arc::<[f32]>::from(vec![0.0; 4096])); 4097];
        compare(&["embedding".into()], &[ColData::Vec(rows)], 4097, &[]);
    }
}

#[cfg(test)]
mod prepared_metadata_tests {
    use super::*;
    use crate::store::Cell;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    #[test]
    fn stale_prepared_metadata_falls_back_to_actual_record() {
        let rows = (0..3)
            .map(|i| Row::from([("body".into(), Cell::Text(format!("body {i}").into()))]))
            .collect::<Vec<_>>();
        for generation in [8, 9] {
            for next_id in [41, 42] {
                for collection in ["other", "docs"] {
                    for rename in [false, true] {
                        for count in [2, 3] {
                            let mut staged_rows = rows[..count].to_vec();
                            if rename {
                                for row in &mut staged_rows {
                                    let value = row.remove("body").unwrap();
                                    row.insert("renamed".into(), value);
                                }
                            }
                            let staged = prepare_insert_columns(
                                generation,
                                next_id,
                                collection,
                                &staged_rows,
                                Vec::new(),
                            );
                            let PreparedEncoding::Ready(encoded) = staged.encoding else {
                                panic!("prepared bytes")
                            };
                            let rec = LogRecord {
                                r#gen: 9,
                                next_id: 42,
                                pack: rows_to_insert_cols("docs", &rows[..2], Vec::new()),
                            };
                            let mut results = Vec::new();
                            for prepared in [None, Some(encoded)] {
                                let path = std::env::temp_dir().join(format!(
                                    "lin-prepared-meta-{}-{}",
                                    std::process::id(),
                                    SEQUENCE.fetch_add(1, Ordering::Relaxed)
                                ));
                                let mut file = OpenOptions::new()
                                    .read(true)
                                    .write(true)
                                    .create_new(true)
                                    .open(&path)
                                    .unwrap();
                                let mut buf = vec![7, 8, 9];
                                let mut bytes = 0;
                                if let Some(encoded) = prepared {
                                    append_prepared_record(
                                        &mut file,
                                        &rec,
                                        encoded,
                                        &mut buf,
                                        SyncMode::Normal,
                                        &mut bytes,
                                    )
                                    .unwrap();
                                } else {
                                    append_record(
                                        &mut file,
                                        &rec,
                                        &mut buf,
                                        SyncMode::Normal,
                                        &mut bytes,
                                    )
                                    .unwrap();
                                }
                                let disk = std::fs::read(&path).unwrap();
                                assert_eq!(bytes, disk.len() as u64);
                                drop(file);
                                std::fs::remove_file(path).unwrap();
                                results.push(disk);
                            }
                            assert_eq!(results[0], results[1]);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod persistent_encoder_queue_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn actual_full_and_disconnected_queues_return_job_and_preserve_encoding() {
        for full in [false, true] {
            let path = std::env::temp_dir().join(format!(
                "lin-encoder-queue-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let (store, mut persist) = crate::store::Store::open_with(
                &path,
                &crate::catalog::fixture(),
                OpenMemOpts::default(),
            )
            .unwrap();
            let (sender, receiver) = mpsc::sync_channel::<EncodeJob>(1);
            persist.encoder = Some(Arc::new(EncoderWorker {
                sender: Some(sender.clone()),
                handle: None,
            }));
            let mut row = Row::new();
            row.insert("id".into(), crate::Cell::text_arc("queue-row"));
            let rows = Arc::new(vec![row]);
            let receiver = if full {
                let (reply, _response) = mpsc::sync_channel(1);
                assert!(
                    sender
                        .try_send(EncodeJob {
                            generation: 1,
                            next_id: 2,
                            collection: "docs".into(),
                            rows: rows.clone(),
                            uniform_keys: false,
                            bytes: Vec::new(),
                            mode: 0,
                            reply
                        })
                        .is_ok()
                );
                Some(receiver)
            } else {
                drop(receiver);
                None
            };
            let rejected = match start_insert_encoding(
                &mut persist,
                1,
                2,
                "docs",
                rows.clone(),
                false,
                Vec::new(),
                0,
            ) {
                Err(job) => job,
                Ok(_) => panic!("unavailable queue accepted job"),
            };
            let staged = rejected.encode();
            drop(receiver);
            assert_eq!(Arc::strong_count(&rows), 1);
            let expected = prepare_insert_columns(1, 2, "docs", &rows, Vec::new());
            let (PreparedEncoding::Ready(actual), PreparedEncoding::Ready(expected)) =
                (staged.encoding, expected.encoding)
            else {
                panic!("encoding failed")
            };
            assert_eq!(actual.magic, expected.magic);
            assert_eq!(actual.bytes, expected.bytes);
            assert_eq!(Arc::try_unwrap(rows).unwrap().len(), 1);
            drop(sender);
            drop(persist);
            drop(store);
            std::fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn last_worker_lease_disconnects_and_joins_thread() {
        let (sender, receiver) = mpsc::sync_channel::<EncodeJob>(1);
        let exited = Arc::new(AtomicBool::new(false));
        let done = exited.clone();
        let handle = std::thread::spawn(move || {
            while let Ok(job) = receiver.recv() {
                job.run();
            }
            done.store(true, Ordering::SeqCst);
        });
        let worker = Arc::new(EncoderWorker {
            sender: Some(sender),
            handle: Some(handle),
        });
        let other = worker.clone();
        drop(worker);
        assert!(!exited.load(Ordering::SeqCst));
        drop(other);
        assert!(exited.load(Ordering::SeqCst));
    }
}

#[cfg(test)]
mod neon_sparse_writer_tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn exact_scalar_bytes_lengths_bits_and_unaligned_mask() {
        let mut state = 0x0badf00d12345678u64;
        let special = [
            0u32, 0x80000000, 0x7f800000, 0xff800000, 0x7fc01234, 0xffc03456, 1, 0x80000001,
        ];
        for offset in 0..4 {
            for len in 0..=1024 {
                let mut values = Vec::new();
                for i in 0..len + offset {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    values.push(f32::from_bits(if i % 3 == 0 {
                        state as u32
                    } else {
                        special[i % special.len()]
                    }));
                }
                #[cfg(target_arch = "aarch64")]
                if len >= 8 && std::arch::is_aarch64_feature_detected!("neon") {
                    let slice = &values[offset..offset + 8];
                    let expected = slice
                        .iter()
                        .enumerate()
                        .fold(0u32, |m, (i, x)| m | (u32::from(x.to_bits() != 0) << i));
                    assert_eq!(unsafe { presence_mask8(slice) }, expected);
                }
                let rows = vec![
                    None,
                    Some(Arc::from([])),
                    Some(Arc::from(&values[offset..])),
                ];
                let mut scalar = Vec::new();
                let mut actual = Vec::new();
                append_sparse_rows_scalar(&rows, &mut scalar).unwrap();
                append_sparse_rows_serial(&rows, &mut actual).unwrap();
                assert_eq!(scalar, actual);
            }
        }
        for dim in [4095, 4096, 4097] {
            let rows = vec![Some(Arc::from(vec![-0.0f32; dim]))];
            let mut scalar = Vec::new();
            let mut actual = Vec::new();
            append_sparse_rows_scalar(&rows, &mut scalar).unwrap();
            append_sparse_rows_serial(&rows, &mut actual).unwrap();
            assert_eq!(scalar, actual);
        }
    }

    #[test]
    fn chunked_parallel_snapshot_roundtrip_and_legacy_compatibility() {
        use std::collections::BTreeMap;
        let mut collections = BTreeMap::new();
        // Collection 1: 5000 rows (spans 3 chunks: 2048, 2048, 904)
        let mut docs = Vec::new();
        for i in 0..5000 {
            let mut r = crate::store::Row::new();
            r.insert("id".into(), crate::store::Cell::text_arc(format!("doc-{}", i)));
            r.insert("num".into(), crate::store::Cell::Int(i as i64));
            docs.push(r);
        }
        collections.insert("docs".to_string(), docs);

        // Collection 2: 10 rows
        let mut users = Vec::new();
        for i in 0..10 {
            let mut r = crate::store::Row::new();
            r.insert("id".into(), crate::store::Cell::text_arc(format!("u-{}", i)));
            users.push(r);
        }
        collections.insert("users".to_string(), users);

        // Collection 3: empty
        collections.insert("empty".to_string(), Vec::new());

        let snap = Snapshot {
            r#gen: 42,
            embed_id: "test-embed".to_string(),
            next_id: 1000,
            catalog_hash: "cat-hash-123".to_string(),
            log_offset: 9999,
            collections: collections.clone(),
            edges: Vec::new(),
            extra_collections: BTreeMap::new(),
            extra_rels: BTreeMap::new(),
            extra_indexes: BTreeMap::new(),
            extra_filters: BTreeMap::new(),
            extra_owned: BTreeMap::new(),
            cold_collections: Vec::new(),
        };

        // 1. Encode with new chunked format
        let bytes = encode_snapshot_bytes(&snap).expect("encode chunked");
        assert!(bytes.len() >= 5);
        assert_eq!(bytes[0], 0x03);
        assert_eq!(&bytes[1..5], &SNAPSHOT_MAGIC);

        // 2. Decode with new reader
        let decoded = decode_snapshot_bytes(&bytes).expect("decode chunked");
        assert_eq!(decoded.r#gen, 42);
        assert_eq!(decoded.embed_id, "test-embed");
        assert_eq!(decoded.next_id, 1000);
        assert_eq!(decoded.catalog_hash, "cat-hash-123");
        assert_eq!(decoded.log_offset, 9999);
        assert_eq!(decoded.collections.len(), 3);
        assert_eq!(decoded.collections["docs"].len(), 5000);
        assert_eq!(decoded.collections["users"].len(), 10);
        assert_eq!(decoded.collections["empty"].len(), 0);
        assert_eq!(
            decoded.collections["docs"][42].get("id"),
            collections["docs"][42].get("id")
        );

        // 3. Test legacy compatibility: encode old monolithic msgpack directly with SNAPSHOT_MAGIC
        let mut legacy_bytes = Vec::new();
        legacy_bytes.extend_from_slice(&SNAPSHOT_MAGIC);
        rmp_serde::encode::write_named(&mut legacy_bytes, &snap).expect("legacy encode");
        let decoded_legacy = decode_snapshot_bytes(&legacy_bytes).expect("decode legacy");
        assert_eq!(decoded_legacy.r#gen, 42);
        assert_eq!(decoded_legacy.collections["docs"].len(), 5000);
        assert_eq!(decoded_legacy.collections["users"].len(), 10);
    }

    #[test]
    #[cfg(feature = "compress-zstd")]
    fn zstd_compressed_snapshot_roundtrip() {
        use std::collections::BTreeMap;
        let mut collections = BTreeMap::new();
        let mut docs = Vec::new();
        for i in 0..1000 {
            let mut r = crate::store::Row::new();
            r.insert("id".into(), crate::store::Cell::text_arc(format!("doc-{}", i)));
            r.insert(
                "content".into(),
                crate::store::Cell::text_arc(
                    "highly compressible repetitive content text data for zstd test",
                ),
            );
            docs.push(r);
        }
        collections.insert("docs".to_string(), docs);

        let snap = Snapshot {
            r#gen: 99,
            embed_id: "embed-zstd".to_string(),
            next_id: 2000,
            catalog_hash: "cat-zstd".to_string(),
            log_offset: 12345,
            collections: collections.clone(),
            edges: Vec::new(),
            extra_collections: BTreeMap::new(),
            extra_rels: BTreeMap::new(),
            extra_indexes: BTreeMap::new(),
            extra_filters: BTreeMap::new(),
            extra_owned: BTreeMap::new(),
            cold_collections: Vec::new(),
        };

        // Monolithic Zstd snapshot (format 0x04)
        let compressed_monolithic =
            encode_snapshot_compressed(&snap).expect("encode monolithic zstd");
        assert_eq!(compressed_monolithic[0], 0x04);
        assert_eq!(&compressed_monolithic[1..5], &SNAPSHOT_MAGIC);
        let decoded_mono =
            decode_snapshot_bytes(&compressed_monolithic).expect("decode monolithic zstd");
        assert_eq!(decoded_mono.r#gen, 99);
        assert_eq!(decoded_mono.collections["docs"].len(), 1000);

        // Chunked parallel Zstd snapshot (format 0x03 with Zstd chunks)
        let chunked_bytes = encode_snapshot_bytes(&snap).expect("encode chunked zstd");
        assert_eq!(chunked_bytes[0], 0x03);
        assert!(
            chunked_bytes.len() < 1000 * 30,
            "Zstd chunk compression must significantly reduce size: got {} bytes",
            chunked_bytes.len()
        );
        let decoded_chunked =
            decode_snapshot_bytes(&chunked_bytes).expect("decode chunked zstd");
        assert_eq!(decoded_chunked.r#gen, 99);
        assert_eq!(decoded_chunked.collections["docs"].len(), 1000);
    }
}
