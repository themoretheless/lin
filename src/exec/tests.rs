use std::sync::Arc;

use crate::ast::{Record, Stmt, Value};
use crate::store::{Cell, Row, content_hash_arc, now_ms, row_text};

use super::Db;
use super::dml::{PREENCODE_TEST_ATTEMPTS, PREENCODE_TEST_MODE, append_batch_shape_rows, record_row};
use super::eval::{LexQuery, lex_score_prepared};

fn append_bulk_worker_rows(
    rows: &mut Vec<Row>,
    records: &[Record],
    now: i64,
    worker: std::io::Result<std::thread::ScopedJoinHandle<'_, Vec<Row>>>,
) {
    match worker {
        Ok(worker) => rows.extend(worker.join().expect("row construction worker panicked")),
        Err(_) => {
            rows.extend(records.iter().map(|record| record_row(record, now)));
        }
    }
}

mod record_tests {
    use super::*;

    #[test]
    fn bulk_short_body_hash_preserves_values_at_batch_and_byte_boundaries() {
        for n in [127, 128, 4095, 4096, 8192] {
            let records = (0..n)
                .map(|i| {
                    let body = match i % 4 {
                        0 => format!("body-{i}"),
                        1 => "repeated body".to_owned(),
                        2 => "λ".repeat(32),
                        _ => "λ".repeat(33),
                    };
                    let mut fields = vec![
                        ("id".into(), Value::String(format!("d-{i}").into())),
                        ("uri".into(), Value::String(format!("bench://{i}").into())),
                        ("title".into(), Value::String("title".into())),
                        ("body".into(), Value::String(body.into())),
                    ];
                    if i % 257 == 0 {
                        fields.push(("hash".into(), Value::String("caller-hash".into())));
                    }
                    Record { fields }
                })
                .collect::<Vec<_>>();
            let mut db = Db::empty();
            db.insert_bulk("docs", &records, &[], false).unwrap();
            let rows = db.store.collection("docs");
            assert_eq!(rows.len(), n);
            for (i, row) in rows.iter().enumerate() {
                let expected = if i % 257 == 0 {
                    Arc::<str>::from("caller-hash")
                } else {
                    content_hash_arc(row["body"].text().unwrap())
                };
                assert_eq!(row["hash"].text(), Some(expected.as_ref()));
            }
        }
    }

    #[test]
    fn batch_shape_matches_reference_and_keeps_source_records() {
        for n in [0, 1, 127, 128, 129, 4095, 4096] {
            let mut records = (0..n)
                .map(|i| Record {
                    fields: vec![
                        ("z".into(), Value::String(format!("v-{i}").into())),
                        ("a".into(), Value::Int(1)),
                        ("t".into(), Value::Now),
                        ("a".into(), Value::Int(i as i64)),
                    ],
                })
                .collect::<Vec<_>>();
            if n > 2 {
                records[n / 2].fields.swap(0, 1);
                records[n - 1]
                    .fields
                    .push(("extra".into(), Value::Bool(true)));
            }
            let before = records.clone();
            for now in [123, 456] {
                let mut actual = vec![Row::from([("prefix".into(), Cell::Int(9))])];
                append_batch_shape_rows(&mut actual, &records, now, false);
                assert_eq!(actual.len(), n + 1);
                assert_eq!(actual[0]["prefix"], Cell::Int(9));
                let expected = records
                    .iter()
                    .map(|r| record_row(r, now))
                    .collect::<Vec<_>>();
                assert_eq!(&actual[1..], expected);
                assert_eq!(records, before);
            }
        }
    }

    #[test]
    fn wide_and_empty_shape_batches_use_reference_semantics() {
        for width in [0, 32, 33, 80] {
            let record = Record {
                fields: (0..width)
                    .map(|i| (format!("f{i}"), Value::Int(i)))
                    .collect(),
            };
            let records = vec![record; 129];
            let mut actual = Vec::new();
            append_batch_shape_rows(&mut actual, &records, 55, false);
            assert_eq!(
                actual,
                records
                    .iter()
                    .map(|r| record_row(r, 55))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn bulk_worker_spawn_failure_preserves_tail_and_timestamp() {
        let left = Record {
            fields: vec![("id".into(), Value::String("left".into()))],
        };
        let right = (0..2053)
            .map(|i| Record {
                fields: vec![
                    ("id".into(), Value::String(format!("right-{i}").into())),
                    ("title".into(), Value::String("old".into())),
                    ("title".into(), Value::String(format!("ЁЖ {i}").into())),
                    ("ts".into(), Value::Now),
                ],
            })
            .collect::<Vec<_>>();
        let mut rows = vec![record_row(&left, 123)];
        append_bulk_worker_rows(
            &mut rows,
            &right,
            123,
            Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "worker unavailable",
            )),
        );
        assert_eq!(rows.len(), 2054);
        assert_eq!(row_text(&rows[0], "id"), Some("left"));
        for (i, row) in rows[1..].iter().enumerate() {
            assert_eq!(row_text(row, "id"), Some(format!("right-{i}").as_str()));
            assert_eq!(row_text(row, "title"), Some(format!("ЁЖ {i}").as_str()));
            assert_eq!(row.get("ts"), Some(&Cell::Time(123)));
        }
    }

    #[test]
    fn parallel_bulk_rows_preserve_order_generated_ids_and_shared_now() {
        for n in [4095, 4096, 4101] {
            let records = (0..n)
                .map(|i| Record {
                    fields: vec![
                        (
                            "uri".into(),
                            Value::String(format!("parallel://{i}").into()),
                        ),
                        ("title".into(), Value::String("old".into())),
                        ("title".into(), Value::String(format!("ЁЖ {i}").into())),
                        ("body".into(), Value::String(format!("body {i}").into())),
                        ("ts".into(), Value::Now),
                    ],
                })
                .collect::<Vec<_>>();
            let mut db = Db::empty().without_embedder();
            let mut reference = Db::empty().without_embedder();
            let expected_ids = (0..n)
                .map(|_| reference.store.alloc_id())
                .collect::<Vec<_>>();
            let before = now_ms();
            let (returned, edges, _) = db.insert_bulk("docs", &records, &[], false).unwrap();
            let after = now_ms();
            assert!(returned.is_empty());
            assert!(edges.is_empty());
            let rows = db.store.collection("docs");
            assert_eq!(rows.len(), n);
            let ts = rows[0].get("ts").unwrap();
            assert!(matches!(ts, Cell::Time(t) if (before..=after).contains(t)));
            for (i, row) in rows.iter().enumerate() {
                assert_eq!(row_text(row, "id"), Some(expected_ids[i].as_str()));
                assert_eq!(row_text(row, "title"), Some(format!("ЁЖ {i}").as_str()));
                assert_eq!(row.get("ts"), Some(ts));
                assert_eq!(
                    row.get("hash"),
                    Some(&Cell::Text(content_hash_arc(&format!("body {i}"))))
                );
                assert!(db.store.get_by_uri(&format!("parallel://{i}")).is_some());
            }
        }
    }

    #[test]
    fn parallel_bulk_checks_duplicate_keys_across_worker_boundary() {
        for field in ["id", "uri"] {
            let mut records = (0..4096)
                .map(|i| Record {
                    fields: vec![
                        ("id".into(), Value::String(format!("id-{i}").into())),
                        (
                            "uri".into(),
                            Value::String(format!("parallel://{i}").into()),
                        ),
                    ],
                })
                .collect::<Vec<_>>();
            let value = records[0]
                .fields
                .iter()
                .find(|(k, _)| k == field)
                .unwrap()
                .1
                .clone();
            records[2048].fields.push((field.into(), value));
            let mut db = Db::empty().without_embedder();
            let error = db.insert_bulk("docs", &records, &[], false).err().unwrap();
            assert!(error.to_string().contains(&format!("duplicate {field}")));
            assert!(db.store.collection("docs").is_empty());
        }
    }

    #[test]
    fn prepared_insert_keeps_last_duplicate_and_dynamic_time() {
        let source = r#"insert docs {title:"old",ts:now,title:"middle",uri:"prepared://duplicate",title:"new",id:"prepared-id"}"#;
        let mut db = Db::empty();
        let prepared = db.prepare(source).unwrap();
        let Stmt::Insert { records, .. } = &prepared.stmts[0] else {
            panic!("insert")
        };
        for now in [123, 456] {
            let row = record_row(&records[0], now);
            assert_eq!(row_text(&row, "title"), Some("new"));
            assert_eq!(row.get("ts"), Some(&Cell::Time(now)));
        }
        let before = now_ms();
        let handle = prepared.run(&mut db).unwrap();
        let after = now_ms();
        assert_eq!(handle.done.n, 1);
        let row = db.store.get_by_id("docs", "prepared-id").unwrap();
        assert_eq!(row_text(row, "title"), Some("new"));
        let Cell::Time(timestamp) = row["ts"] else {
            panic!("time")
        };
        assert!((before..=after).contains(&timestamp));
    }

    #[test]
    fn prepared_literal_storage_is_shared_and_outlives_source_and_plan() {
        let mut db = Db::empty();
        let source = String::from(
            r#"insert docs {id:"shared",uri:"shared://doc",title:"Shared ЁЖ 🦔",body:"body"}"#,
        );
        let prepared = db.prepare(&source).unwrap();
        drop(source);
        let Stmt::Insert { records, .. } = &prepared.stmts[0] else {
            panic!("insert")
        };
        let Value::String(literal) = &records[0]
            .fields
            .iter()
            .find(|(name, _)| name == "title")
            .unwrap()
            .1
        else {
            panic!("literal")
        };
        let literal = std::sync::Arc::clone(literal);
        assert_eq!(prepared.run(&mut db).unwrap().done.n, 1);
        let Cell::Text(stored) = &db.store.get_by_id("docs", "shared").unwrap()["title"] else {
            panic!("text")
        };
        assert!(std::sync::Arc::ptr_eq(stored, &literal));
        drop(prepared);
        drop(literal);
        assert_eq!(
            row_text(db.store.get_by_uri("shared://doc").unwrap(), "title"),
            Some("Shared ЁЖ 🦔")
        );
    }

    #[test]
    fn lexical_lowercase_preserves_ascii_unicode_context_and_field_phrases() {
        for (title, body, query, expected) in [
            ("wal note", "body", "wal", 6),
            ("WAL NOTE", "BODY", "wal", 6),
            ("ΟΣ", "BODY", "ος", 6),
            ("ΟΣ", "BODY", "οσ", 0),
            ("one", "two", "one two", 9),
        ] {
            let row = Row::from([
                ("title".into(), Cell::text_arc(title)),
                ("body".into(), Cell::text_arc(body)),
            ]);
            assert_eq!(
                lex_score_prepared(&row, &LexQuery::new(query)),
                expected,
                "{title}/{query}"
            );
        }
    }

    #[test]
    fn row_build_preserves_last_duplicate_and_execution_time() {
        let record = Record {
            fields: vec![
                ("z".into(), Value::String("old".into())),
                ("a".into(), Value::Now),
                ("z".into(), Value::String("new".into())),
                ("m".into(), Value::Int(42)),
            ],
        };
        let row = record_row(&record, 123);
        assert_eq!(row.len(), 3);
        assert_eq!(row_text(&row, "z"), Some("new"));
        assert_eq!(row.get("a"), Some(&Cell::Time(123)));
        assert_eq!(record_row(&record, 456).get("a"), Some(&Cell::Time(456)));
    }

    #[test]
    fn prepared_param_binding_executes_queries_and_dml_without_reparse() {
        let mut db = Db::empty();
        db.run("col accounts { balance: i64 }").unwrap();
        let ins = db
            .prepare("insert accounts { id: $id, balance: $bal }")
            .unwrap();
        assert!(ins.has_params);
        ins.execute_with(&mut db, &[("id", Cell::text_arc("a1")), ("bal", Cell::Int(100))])
            .unwrap();
        ins.execute_with(&mut db, &[("id", Cell::text_arc("a2")), ("bal", Cell::Int(250))])
            .unwrap();

        let sel = db
            .prepare("accounts | id == $id | { balance }")
            .unwrap();
        assert_eq!(
            sel.scalar_with(&mut db, &[("id", Cell::text_arc("a2"))])
                .unwrap(),
            Cell::Int(250)
        );

        let upd = db
            .prepare("update accounts[id == $id] cas each { balance: $bal }")
            .unwrap();
        {
            let mut tx = db.transaction().unwrap();
            tx.execute_with(&upd, &[("id", Cell::text_arc("a1")), ("bal", Cell::Int(999))])
                .unwrap();
            tx.commit().unwrap();
        }
        assert_eq!(
            sel.scalar_with(&mut db, &[("id", Cell::text_arc("a1"))])
                .unwrap(),
            Cell::Int(999)
        );
    }
}

mod grouping_tests {
    use crate::Db;
    use crate::store::Cell;

    #[test]
    fn mixed_numeric_and_text_grouping_preserves_cell_identity_across_operators() {
        let mut db = Db::empty();
        db.run("col items { k: f64, v: i64, tag: text }").unwrap();
        db.run(
            r#"insert items [
                { id: "i1", k: 1, v: 10, tag: "active" },
                { id: "i2", k: 1.0, v: 20, tag: "active" },
                { id: "i3", k: 0.0, v: 30, tag: "active" },
                { id: "i4", k: -0.0, v: 40, tag: "active" },
                { id: "i5", v: 50, tag: "active" }
            ]"#,
        )
        .unwrap();

        let unfiltered = db.run("items | count by k").unwrap().into_rows();
        let filtered = db
            .run(r#"items | tag == "active" | count by k"#)
            .unwrap()
            .into_rows();
        assert_eq!(unfiltered, filtered);
        assert_eq!(unfiltered.len(), 5);
        assert_eq!(unfiltered[0]["k"], Cell::Null);

        let sums = db.run("items | sum v by k").unwrap().into_rows();
        assert_eq!(sums.len(), 5);

        let win = db
            .run("items | window row_number by k as rn")
            .unwrap()
            .into_rows();
        assert_eq!(win.len(), 5);
        for row in &win {
            assert_eq!(row["rn"], Cell::Int(1));
        }
    }
}

mod preencoded_insert_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    struct Mode(u8);
    impl Mode {
        fn new(mode: u8) -> Self {
            PREENCODE_TEST_ATTEMPTS.with(|x| x.set(0));
            Self(PREENCODE_TEST_MODE.with(|x| x.replace(mode)))
        }
    }
    impl Drop for Mode {
        fn drop(&mut self) {
            PREENCODE_TEST_MODE.with(|x| x.set(self.0));
        }
    }
    fn open() -> (Temp, Db) {
        let p = std::env::temp_dir().join(format!(
            "lin-preencoded-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let db = Db::open(&p).unwrap();
        (Temp(p), db)
    }
    fn source(n: usize) -> String {
        format!(
            "insert docs [{}]",
            (0..n)
                .map(|i| format!(
                    r#"{{id:"i{i}",uri:"bench://{i}",title:"title {i}",body:"body {i}",wing:"rag",ts:timestamp(1700000000000)}}"#
                ))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
    #[test]
    fn shared_worker_survives_encoder_panic_and_one_database_closing() {
        let (_first, mut first) = open();
        let (_second, mut second) = open();
        first.run("index docs [wing, ts]").unwrap();
        second.run("index docs [wing, ts]").unwrap();
        {
            let _mode = Mode::new(2);
            first.quotas.max_rows = 0;
            assert!(
                first
                    .run(&source(4096))
                    .unwrap_err()
                    .to_string()
                    .contains("quota:")
            );
        }
        second.run(&source(4096)).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            first.persist.as_ref().unwrap().encoder.as_ref().unwrap(),
            second.persist.as_ref().unwrap().encoder.as_ref().unwrap()
        ));
        drop(first);
        let next = source(4096)
            .replace("id:\"i", "id:\"j")
            .replace("bench://", "repeat://");
        second.run(&next).unwrap();
        assert_eq!(second.store.collection("docs").len(), 8192);
        let read = Db::open_read(&_second.0).unwrap();
        assert_eq!(
            read.run("docs | take 9000").unwrap().rows,
            second.run("docs | take 9000").unwrap().rows
        );
        drop(read);
        drop(second);
    }
    #[test]
    fn worker_spawn_fallback_matches_normal_values_and_readonly_replay() {
        for n in [683, 1000, 4096] {
            for mode in [0, 1, 4, 5] {
                let (_dir, mut db) = open();
                db.run("index docs [wing, ts]").unwrap();
                let _mode = Mode::new(mode);
                let src = source(n);
                assert_eq!(db.run(&src).unwrap().done.n, n);
                assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 1);
                let mut expected = Db::empty();
                expected.run("index docs [wing, ts]").unwrap();
                expected.run(&src).unwrap();
                let query = "docs | {id, uri, title, body, wing, ts, hash, embedding} | take 5000";
                let a = db.run(query).unwrap().into_rows();
                let b = expected.run(query).unwrap().into_rows();
                assert_eq!(a.len(), n);
                assert_eq!(a, b);
                let read = Db::open_read(&_dir.0).unwrap();
                assert_eq!(read.run(query).unwrap().into_rows(), a);
                drop(read);
                drop(db);
            }
        }
    }
    #[test]
    fn exact_small_float_budget_can_start_at_512_rows() {
        let (_dir, db) = open();
        let mut db = db.with_embedder(std::sync::Arc::new(crate::embed::HashingEmbedder::new(
            "test/1024",
            1024,
        )));
        let _mode = Mode::new(0);
        db.run(&source(512)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 1);
        drop(db);
    }
    #[test]
    fn small_work_hint_is_conservative_for_mixed_vector_dimensions() {
        struct SkewEmbedder {
            first: usize,
            rest: usize,
        }
        fn vector(n: usize) -> Arc<[f32]> {
            let mut values = vec![0.0; n];
            values[0] = 1.0;
            values[1] = -0.0;
            values[2] = f32::from_bits(1);
            values.into()
        }
        impl crate::embed::Embedder for SkewEmbedder {
            fn id(&self) -> &str {
                "test/skew"
            }
            fn dim(&self) -> usize {
                self.rest
            }
            fn embed(&self, _: &str) -> Arc<[f32]> {
                vector(self.rest)
            }
            fn embed_batch(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
                let first = vector(self.first);
                let rest = vector(self.rest);
                (0..texts.len())
                    .map(|i| if i == 0 { first.clone() } else { rest.clone() })
                    .collect()
            }
        }
        for (first, rest, attempts) in [(1024, 8, 0), (8, 1024, 0), (1024, 1024, 1)] {
            let (_dir, db) = open();
            let mut db = db.with_embedder(Arc::new(SkewEmbedder { first, rest }));
            let _mode = Mode::new(0);
            db.run(&source(683)).unwrap();
            assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), attempts);
            let verify = |rows: &[Row]| {
                assert_eq!(rows.len(), 683);
                for (i, row) in rows.iter().enumerate() {
                    let values = row["embedding"].as_vec().unwrap();
                    assert_eq!(values.len(), if i == 0 { first } else { rest });
                    assert_eq!(values[0].to_bits(), 1.0f32.to_bits());
                    assert_eq!(values[1].to_bits(), (-0.0f32).to_bits());
                    assert_eq!(values[2].to_bits(), 1);
                    assert!(values[3..].iter().all(|v| v.to_bits() == 0));
                }
            };
            verify(db.store.collection("docs"));
            let read = Db::open_read(&_dir.0).unwrap();
            verify(&read.run("docs | take 1000").unwrap().rows);
            drop(read);
            drop(db);
        }
    }
    #[test]
    fn encoded_errors_defer_until_quota_and_index_checks() {
        for mode in [2, 3] {
            let (_dir, mut db) = open();
            db.run("index docs [wing, ts]").unwrap();
            let _mode = Mode::new(mode);
            db.quotas.max_rows = 0;
            let error = db.run(&source(683)).err().unwrap();
            assert!(error.to_string().contains("quota:"), "{error}");
            assert!(db.store.collection("docs").is_empty());
            assert!(db.store.row_index("docs", "i0").is_none());
            drop(db);
            let (_dir, mut db) = open();
            db.run("index docs unique [wing]").unwrap();
            let _mode = Mode::new(mode);
            let error = db.run(&source(683)).err().unwrap();
            assert!(error.to_string().contains("unique index"), "{error}");
            assert!(db.store.collection("docs").is_empty());
            drop(db);
        }
        let (_dir, mut db) = open();
        db.run("index docs [wing, ts]").unwrap();
        let _mode = Mode::new(3);
        let before = std::fs::read(_dir.0.join("log")).unwrap();
        let error = db.run(&source(683)).err().unwrap();
        assert!(error.to_string().contains("injected WAL encoder failure"));
        assert!(db.store.collection("docs").is_empty());
        assert_eq!(std::fs::read(_dir.0.join("log")).unwrap(), before);
        drop(db);
    }
    #[test]
    fn noneligible_native_small_and_explicit_transactions_stay_serial() {
        let _mode = Mode::new(3);
        let mut memory = Db::empty();
        memory.run(&source(4096)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        let (_dir, mut db) = open();
        db.run(&source(127)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
        let (_dir, mut db) = open();
        db.run(&source(511)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
        for n in [512, 682] {
            let (_dir, mut db) = open();
            db.run(&source(n)).unwrap();
            assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
            drop(db);
        }
        let (_dir, db) = open();
        let mut db = db.with_embedder(std::sync::Arc::new(crate::embed::HashingEmbedder::new(
            "test/2048",
            2048,
        )));
        db.run(&source(511)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
        let (_dir, db) = open();
        let mut db = db.without_embedder();
        db.run(&source(4096)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
        let (_dir, db) = open();
        let mut db = db.with_embedder(std::sync::Arc::new(crate::embed::HashingEmbedder::new(
            "test/64", 64,
        )));
        db.run(&source(4096)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
        let (_dir, mut db) = open();
        db.run(&source(16385)).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
        let (_dir, mut db) = open();
        let prep = db.prepare(&source(4096)).unwrap();
        {
            let mut tx = db.transaction().unwrap();
            tx.run_prepared(&prep).unwrap();
            tx.commit().unwrap();
        }
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
        let (_dir, mut db) = open();
        let mut statements = crate::parse::parse_program(&source(4096)).unwrap();
        statements.extend(crate::parse::parse_program("docs | count").unwrap());
        let prepared = db.prepare_stmts(statements).unwrap();
        db.run_prepared(&prepared).unwrap();
        assert_eq!(PREENCODE_TEST_ATTEMPTS.with(|x| x.get()), 0);
        drop(db);
    }

    #[test]
    fn test_point_delete_and_update_extracted() {
        let mut db = Db::empty();
        let prep = db.prepare(r#"delete docs[id == "d-20"] cas "h:123""#).unwrap();
        assert!(prep.point_delete.is_some());
        let prep_up = db
            .prepare(r#"update docs[id == "d-20"] cas "h:123" { wing: "sys" }"#)
            .unwrap();
        assert!(prep_up.point_update.is_some());
    }
}
