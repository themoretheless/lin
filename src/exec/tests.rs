use std::sync::Arc;

use crate::ast::{Record, Stmt, Value};
use crate::store::{Cell, Row, content_hash_arc, now_ms, row_text};

use super::Db;
use super::dml::{
    PREENCODE_TEST_ATTEMPTS, PREENCODE_TEST_MODE, append_batch_shape_rows, record_row,
};
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
                let mut actual = vec![Row::from([("prefix", Cell::Int(9))])];
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
                ("title", Cell::text_arc(title)),
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
        ins.execute_with(
            &mut db,
            &[("id", Cell::text_arc("a1")), ("bal", Cell::Int(100))],
        )
        .unwrap();
        ins.execute_with(
            &mut db,
            &[("id", Cell::text_arc("a2")), ("bal", Cell::Int(250))],
        )
        .unwrap();

        let sel = db.prepare("accounts | id == $id | { balance }").unwrap();
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
            tx.execute_with(
                &upd,
                &[("id", Cell::text_arc("a1")), ("bal", Cell::Int(999))],
            )
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
        let prep = db
            .prepare(r#"delete docs[id == "d-20"] cas "h:123""#)
            .unwrap();
        assert!(prep.point_delete.is_some());
        let prep_up = db
            .prepare(r#"update docs[id == "d-20"] cas "h:123" { wing: "sys" }"#)
            .unwrap();
        assert!(prep_up.point_update.is_some());
    }

    #[test]
    fn test_batch_into_rows_and_to_rows_widths() {
        use crate::batch::{Column, RecordBatch};
        for width in [1usize, 2, 3, 4, 5, 8] {
            let names: std::sync::Arc<[String]> = (0..width)
                .rev()
                .map(|c| format!("col_{c}"))
                .collect::<Vec<_>>()
                .into();
            let n = 1100;
            let cols: Vec<Column> = (0..width)
                .map(|c| {
                    if c % 2 == 0 {
                        Column::from_ints((0..n as i64).map(|i| i * 10 + c as i64).collect())
                    } else {
                        Column::Cells(
                            (0..n)
                                .map(|i| Cell::from_text(format!("v-{c}-{i}")))
                                .collect(),
                        )
                    }
                })
                .collect();
            let batch = RecordBatch { names, cols };
            let via_to_rows = batch.to_rows();
            let via_into_rows = batch.clone().into_rows();
            assert_eq!(via_to_rows, via_into_rows);
            crate::store::release_general_rows(via_into_rows);
            let via_reused = batch.into_rows();
            assert_eq!(via_to_rows, via_reused);
        }
    }

    #[test]
    fn test_insert_batch_one_pass_hash_and_embed_matches_stmt_insert() {
        use crate::batch::{Column, RecordBatch};
        let embedder = std::sync::Arc::new(crate::embed::HashingEmbedder::new("test/32", 32));
        let mut db_stmt = Db::empty().with_embedder(embedder);
        let n = 300usize;

        let rows_src: Vec<String> = (0..n)
            .map(|i| {
                format!(
                    r#"{{ id: "doc-{i:04}", uri: "lin://test/doc-{i:04}", title: "Title number {i}", body: "Body payload for document {i} with tokens", wing: "rag", ts: timestamp(1700000000000) }}"#
                )
            })
            .collect();
        db_stmt
            .run(&format!("insert docs [{}]", rows_src.join(",")))
            .unwrap();

        let batch2 = RecordBatch {
            names: vec![
                "id".into(),
                "uri".into(),
                "title".into(),
                "body".into(),
                "wing".into(),
                "ts".into(),
            ]
            .into(),
            cols: vec![
                Column::Cells((0..n).map(|i| Cell::from_text(format!("doc-{i:04}"))).collect()),
                Column::Cells(
                    (0..n)
                        .map(|i| Cell::from_text(format!("lin://test/doc-{i:04}")))
                        .collect(),
                ),
                Column::Cells((0..n).map(|i| Cell::from_text(format!("Title number {i}"))).collect()),
                Column::Cells(
                    (0..n)
                        .map(|i| Cell::from_text(format!("Body payload for document {i} with tokens")))
                        .collect(),
                ),
                Column::Cells((0..n).map(|_| Cell::from_text("rag")).collect()),
                (0..n).map(|_| Cell::Time(1_700_000_000_000)).collect(),
            ],
        };
        let mut db_batch2 = Db::empty().with_embedder(std::sync::Arc::new(
            crate::embed::HashingEmbedder::new("test/32", 32),
        ));
        assert_eq!(db_batch2.insert_batch("docs", &batch2).unwrap().n, n);
        assert!(db_batch2.insert_batch("docs", &batch2).is_err());

        let r_batch = db_batch2
            .run("docs | sort id asc | { id, uri, hash, embedding } | take all")
            .unwrap()
            .rows
            .clone();
        let r_stmt = db_stmt
            .run("docs | sort id asc | { id, uri, hash, embedding } | take all")
            .unwrap()
            .rows
            .clone();
        assert_eq!(r_batch, r_stmt);
    }

    #[test]
    fn test_window_without_projection_matches_projected_and_empty() {
        let mut db = Db::empty();
        db.run(
            r#"
            insert users [{ id: "u1", email: "u1@x.io" }, { id: "u2", email: "u2@x.io" }]
            insert orders [
                { id: "o1", user_id: "u1", total: 10.5 },
                { id: "o2", user_id: "u1", total: 20.0 },
                { id: "o3", user_id: "u2", total: 7.25 }
            ]
            col metrics { id: text, grp: text, v: i64 }
            insert metrics [
                { id: "m1", grp: "a", v: 5 },
                { id: "m2", grp: "a", v: 15 },
                { id: "m3", grp: "b", v: 9 }
            ]
            "#,
        )
        .unwrap();

        let unproj = db
            .run("orders | window sum total by user_id sort id asc as running | take all")
            .unwrap()
            .rows
            .clone();
        let proj = db
            .run("orders | window sum total by user_id sort id asc as running | { id, running, total, user_id } | take all")
            .unwrap()
            .rows
            .clone();
        assert_eq!(unproj, proj);

        let batch_unproj = db
            .run_batch("orders | window sum total by user_id sort id asc as running | take all")
            .unwrap();
        assert_eq!(batch_unproj.to_rows(), proj);

        let m_unproj = db
            .run("metrics | v > 0 | window row_number by grp sort id asc as rn | sort id asc | take all")
            .unwrap()
            .rows
            .clone();
        let m_proj = db
            .run("metrics | v > 0 | window row_number by grp sort id asc as rn | sort id asc | { grp, id, rn, v } | take all")
            .unwrap()
            .rows
            .clone();
        assert_eq!(m_unproj, m_proj);

        let empty_batch = db
            .run_batch("metrics | v > 9999 | window row_number by grp sort id asc as rn | take all")
            .unwrap();
        assert_eq!(empty_batch.n(), 0);
        assert!(empty_batch.names.is_empty());
    }

    #[test]
    fn test_parallel_filter_scan_and_counts_above_threshold() {
        use crate::batch::{Column, RecordBatch};
        let mut db = Db::empty();
        db.run("col big { id: text, grp: text, val: i64 }").unwrap();
        let n = 70_000usize;
        let batch = RecordBatch {
            names: vec!["id".into(), "grp".into(), "val".into()].into(),
            cols: vec![
                Column::Cells((0..n).map(|i| Cell::from_text(format!("k-{i:06}"))).collect()),
                Column::Cells((0..n).map(|i| Cell::from_text(format!("g-{}", i % 7))).collect()),
                Column::from_ints((0..n as i64).collect()),
            ],
        };
        db.insert_batch("big", &batch).unwrap();

        let cnt = db.run("big | val >= 10000 | count").unwrap().rows.clone();
        assert_eq!(cnt[0].get("hits"), Some(&Cell::Int(60_000)));

        let by = db
            .run("big | val < 7000 | count by grp")
            .unwrap()
            .rows
            .clone();
        assert_eq!(by.len(), 7);
        for row in &by {
            assert_eq!(row.get("hits"), Some(&Cell::Int(1000)));
        }

        let scanned = db
            .run("big | val >= 69995 | { id, val } | take all")
            .unwrap()
            .rows
            .clone();
        assert_eq!(scanned.len(), 5);
        assert_eq!(scanned[0].get("val"), Some(&Cell::Int(69_995)));
        assert_eq!(scanned[4].get("val"), Some(&Cell::Int(69_999)));
    }

    #[test]
    fn test_hot_left_cold_right_join_and_get_many_projected_by_id() {
        let dir = Temp(std::env::temp_dir().join(format!(
            "lin-cold-join-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        let opts = crate::OpenOpts {
            sync: crate::persist::SyncMode::Normal,
            cold: true,
        };
        let mut db = Db::open_with(&dir.0, opts).unwrap();
        let users_src: Vec<String> = (0..80)
            .map(|i| format!(r#"{{ id: "u-{i:03}", email: "u{i}@example.com" }}"#))
            .collect();
        db.run(&format!("insert users [{}]", users_src.join(",")))
            .unwrap();
        let orders_src: Vec<String> = (0..20)
            .map(|i| {
                let uid = format!("u-{:03}", (i * 3) % 80);
                format!(
                    r#"{{ id: "o-{i:02}", user_id: "{uid}", total: {}.5 }}"#,
                    i + 1
                )
            })
            .collect();
        db.run(&format!("insert orders [{}]", orders_src.join(",")))
            .unwrap();
        db.close().unwrap();

        let mut db = Db::open_with(&dir.0, opts).unwrap();
        assert!(db.store.is_disk_backed("users"));
        assert!(!db.store.is_disk_backed("orders"));

        {
            let eng = db.store.disk_engine_read().unwrap();
            let got = eng
                .get_many_projected_by_id(
                    "users",
                    ["u-075", "u-000", "u-999", "u-033", "u-000"],
                    &["email"],
                )
                .unwrap();
            assert_eq!(got.len(), 3);
            assert_eq!(
                got["u-033"].get("email").and_then(Cell::text),
                Some("u33@example.com")
            );
            assert!(got["u-033"].get("id").is_none());
        }

        let joined = db
            .run("orders | join users on user_id | sort id asc | { id, users.email } | take all")
            .unwrap()
            .rows
            .clone();
        assert_eq!(joined.len(), 20);
        assert_eq!(
            joined[0].get("users.email").and_then(Cell::text),
            Some("u0@example.com")
        );
        assert_eq!(
            joined[1].get("users.email").and_then(Cell::text),
            Some("u3@example.com")
        );

        // Verify lazy cursor over hot-left ⋈ cold-right streams without hydrating cold `users`.
        let stmts = crate::parse::parse_program(
            "orders | join users on user_id | { id, users.email } | take 5",
        )
        .unwrap();
        let crate::ast::Stmt::Query(q) = &stmts[0] else {
            panic!("expected query");
        };
        let cur = db.cursor(q).unwrap();
        assert!(cur.is_lazy());
        let cursor_rows: Vec<Row> = cur.map(Result::unwrap).collect();
        assert_eq!(cursor_rows.len(), 5);
        assert!(
            db.store
                .disk
                .as_ref()
                .unwrap()
                .tables["users"]
                .materialized
                .get()
                .is_none(),
            "cold right table must stay unmaterialized during lazy cursor join"
        );
    }

    #[test]
    fn test_reembed_collection_wal_durability_and_embed_id_guard() {
        let dir = Temp(std::env::temp_dir().join(format!(
            "lin-reembed-wal-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        let emb_a = std::sync::Arc::new(crate::embed::HashingEmbedder::new("model-a/16", 16));
        let emb_b = std::sync::Arc::new(crate::embed::HashingEmbedder::new("model-b/32", 32));
        {
            let mut db = Db::open(&dir.0).unwrap().with_embedder(emb_a);
            db.run(
                r#"insert docs [
                    { id: "d1", uri: "lin://re/1", title: "distributed consensus", body: "raft wal" },
                    { id: "d2", uri: "lin://re/2", title: "columnar storage", body: "zstd mmap" }
                ]"#,
            )
            .unwrap();
            db.close().unwrap();
        }
        let expected_d1 = crate::embed::Embedder::embed(&*emb_b, "distributed consensus raft wal");
        {
            // Swap embedder on populated DB without reembedding: vector search must refuse.
            let mut db = Db::open(&dir.0).unwrap().with_embedder(emb_b.clone());
            let err = db
                .run(r#"docs | search vec "raft" | { id } | take 5"#)
                .unwrap_err();
            assert!(err.to_string().contains("embed_id mismatch"), "{err}");

            // Reembed logs to WAL and updates head; drop WITHOUT calling close/checkpoint!
            assert_eq!(db.reembed_collection("docs").unwrap(), 2);
            let hits = db
                .run(r#"docs | search vec "raft" | { id } | take 5"#)
                .unwrap()
                .rows
                .clone();
            assert!(!hits.is_empty());
        }
        // Simulate crash recovery: reopen WITHOUT checkpoint must preserve 32-dim embeddings.
        let mut recovered = Db::open(&dir.0).unwrap().with_embedder(emb_b);
        let rows = recovered
            .run(r#"docs | id == "d1" | { id, embedding }"#)
            .unwrap()
            .rows
            .clone();
        let got_vec = rows[0].get("embedding").and_then(Cell::as_vec).unwrap();
        assert_eq!(got_vec.len(), 32);
        assert_eq!(got_vec, expected_d1.as_ref());
    }

    #[test]
    fn test_cold_index_eq_late_populates_hot_cache() {
        let dir = Temp(std::env::temp_dir().join(format!(
            "lin-late-idx-cache-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        let opts = crate::OpenOpts {
            sync: crate::persist::SyncMode::Normal,
            cold: true,
        };
        let mut db = Db::open_with(&dir.0, opts).unwrap();
        db.run("col logs { id: text, svc: text, code: i64 }").unwrap();
        db.run("index logs [svc]").unwrap();
        let rows_src: Vec<String> = (0..120)
            .map(|i| {
                format!(
                    r#"{{ id: "l-{i:03}", svc: "{}", code: {} }}"#,
                    if i % 3 == 0 { "auth" } else { "api" },
                    200 + (i % 5) * 100
                )
            })
            .collect();
        db.run(&format!("insert logs [{}]", rows_src.join(",")))
            .unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();

        let mut db = Db::open_with(&dir.0, opts).unwrap();
        assert!(db.store.is_disk_backed("logs"));
        let r1 = db
            .run(r#"logs | svc == "auth" | code >= 400 | sort id asc | { id, code } | take all"#)
            .unwrap()
            .rows
            .clone();
        let hits_before = db
            .store
            .disk_engine_read()
            .unwrap()
            .hot_cache_stats()
            .sec_query_hits;
        let r2 = db
            .run(r#"logs | svc == "auth" | code >= 400 | sort id asc | { id, code } | take all"#)
            .unwrap()
            .rows
            .clone();
        let hits_after = db
            .store
            .disk_engine_read()
            .unwrap()
            .hot_cache_stats()
            .sec_query_hits;
        assert_eq!(r1, r2);
        assert!(
            hits_after > hits_before,
            "expected HotTierCache sec_query_hits to advance"
        );
    }

    #[test]
    fn cold_chunked_insert_batch_stays_on_disk_and_reopens() {
        let dir = Temp(std::env::temp_dir().join(format!(
            "lin-cold-chunked-batch-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        let opts = crate::OpenOpts {
            cold: true,
            sync: crate::persist::SyncMode::Normal,
        };
        let mut db = Db::open_with(&dir.0, opts).unwrap();
        db.run("col big { id: text, grp: text, val: i64 }").unwrap();
        let seeds: Vec<String> = (0..40)
            .map(|i| format!(r#"{{ id: "seed-{i:02}", grp: "g0", val: {i} }}"#))
            .collect();
        db.run(&format!("insert big [{}]", seeds.join(",")))
            .unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();

        let mut db = Db::open_with(&dir.0, opts).unwrap();
        db.quotas.max_rows = 2_000_000;
        assert!(db.store.is_disk_backed("big"));

        // 150_000 rows > COLD_WAL_CHUNK_ROWS (131_072), forcing multi-frame WAL commit
        // without promoting `big` into memory.
        let n = 150_000usize;
        let ids: Vec<Cell> = (0..n).map(|i| Cell::from_text(&format!("b-{i:06}"))).collect();
        let grps: Vec<Cell> = (0..n)
            .map(|i| Cell::from_text(if i % 2 == 0 { "even" } else { "odd" }))
            .collect();
        let vals: Vec<i64> = (0..n).map(|i| i as i64).collect();
        let batch = crate::batch::RecordBatch {
            names: std::sync::Arc::from([
                "id".to_string(),
                "grp".to_string(),
                "val".to_string(),
            ]),
            cols: vec![
                crate::batch::Column::Cells(ids),
                crate::batch::Column::Cells(grps),
                crate::batch::Column::Int {
                    vals,
                    valid: crate::batch::Validity::all_valid(n),
                },
            ],
        };
        let done = db.insert_batch("big", &batch).unwrap();
        assert_eq!(done.n, n);
        assert!(
            db.store.is_disk_backed("big")
                && !db.store.disk_rows_materialized("big")
                && db.store.collections.get("big").is_none_or(Vec::is_empty),
            "cold table must not promote into RAM on chunked insert_batch"
        );
        assert_eq!(
            db.run("big | count").unwrap().rows[0].get("hits"),
            Some(&Cell::Int((n + 40) as i64))
        );
        db.close().unwrap();

        let mut reopened = Db::open_with(&dir.0, opts).unwrap();
        assert!(reopened.store.is_disk_backed("big"));
        assert_eq!(
            reopened.run("big | count").unwrap().rows[0].get("hits"),
            Some(&Cell::Int((n + 40) as i64))
        );
    }

    #[test]
    fn cold_lazy_search_and_hop_cursors_stay_unhydrated() {
        let dir = Temp(std::env::temp_dir().join(format!(
            "lin-cold-lazy-search-hop-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        let opts = crate::OpenOpts {
            cold: true,
            sync: crate::persist::SyncMode::Normal,
        };
        let mut db = Db::open_with(&dir.0, opts).unwrap();
        db.run(
            r#"
            insert docs { id: "d1", uri: "lin://a", title: "Alpha Rust", body: "columnar vector engine", layer: "raw" } with edge wikilink -> "d2"
            insert docs { id: "d2", uri: "lin://b", title: "Beta Storage", body: "mmap segment index", layer: "raw" }
            insert docs { id: "d3", uri: "lin://c", title: "Gamma Rust", body: "hybrid lexical vector search", layer: "raw" }
            "#,
        )
        .unwrap();
        let pad: Vec<String> = (4..45)
            .map(|i| format!(r#"{{ id: "d{i}", uri: "lin://p{i}", title: "Pad {i}", body: "filler document {i}", layer: "raw" }}"#))
            .collect();
        db.run(&format!("insert docs [{}]", pad.join(","))).unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();

        let db = Db::open_with(&dir.0, opts).unwrap();
        assert!(db.store.is_disk_backed("docs"));

        for q_src in [
            r#"docs | search lex "rust" | { id, title } | take 2"#,
            r#"docs | search "vector engine" | { id, title } | take 2"#,
            r#"docs | search vec "mmap segment" | { id, title } | take 2"#,
            r#"docs | id == "d1" | hop wikilink | { id, title } | take 5"#,
        ] {
            let crate::Stmt::Query(q) = crate::parse(q_src).unwrap() else {
                panic!("expected query");
            };
            let cur = crate::QueryCursor::open_db(&db, &q).unwrap();
            assert!(cur.is_lazy(), "expected lazy cursor for `{q_src}`");
            let rows: Vec<_> = cur.collect::<Result<Vec<_>, _>>().unwrap();
            assert!(!rows.is_empty(), "expected rows for `{q_src}`");
            assert!(
                !db.store.disk_rows_materialized("docs"),
                "`docs` materialized during `{q_src}`"
            );
        }
    }

    #[test]
    fn multi_sort_and_cold_unprojected_window_match_row_pipeline() {
        let dir = Temp(std::env::temp_dir().join(format!(
            "lin-cold-win-unproj-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        let opts = crate::OpenOpts {
            cold: true,
            sync: crate::persist::SyncMode::Normal,
        };
        let mut cold_db = Db::open_with(&dir.0, opts).unwrap();
        let mut hot_db = Db::empty();
        let recs: Vec<String> = (0..40)
            .map(|i| {
                format!(
                    r#"{{ id: "m{i:02}", dept: "{}", score: {} }}"#,
                    if i % 2 == 0 { "eng" } else { "ops" },
                    10 + (i % 5) * 10
                )
            })
            .collect();
        let setup = format!(
            "col metrics {{ id: text, dept: text, score: i64 }}\ninsert metrics [{}]",
            recs.join(",")
        );
        cold_db.run(&setup).unwrap();
        hot_db.run(&setup).unwrap();
        cold_db.checkpoint().unwrap();
        cold_db.close().unwrap();

        let mut cold_db = Db::open_with(&dir.0, opts).unwrap();
        assert!(cold_db.store.is_disk_backed("metrics"));

        let q = "metrics | window rank by dept sort score desc as rnk | sort rnk asc | sort id desc | take all";
        let cold_rows = cold_db.run(q).unwrap().into_rows();
        assert!(
            !cold_db.store.disk_rows_materialized("metrics"),
            "cold unprojected window should run columnar without hydrating `metrics`"
        );
        let hot_rows = hot_db.run(q).unwrap().into_rows();
        assert_eq!(cold_rows, hot_rows);
    }

    #[test]
    fn import_backup_into_checked_refuses_non_empty_without_overwrite() {
        let src_dir = Temp(std::env::temp_dir().join(format!(
            "lin-bkp-src-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        let dst_dir = Temp(std::env::temp_dir().join(format!(
            "lin-bkp-dst-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        std::fs::create_dir_all(&dst_dir.0).unwrap();
        let bkp_file = src_dir.0.join("out.backup");

        let mut db = Db::open_with(
            &src_dir.0,
            crate::OpenOpts {
                cold: false,
                sync: crate::persist::SyncMode::Normal,
            },
        )
        .unwrap();
        db.run(r#"insert docs { id: "b1", uri: "lin://b1", title: "Backup" }"#)
            .unwrap();
        db.export_backup(&bkp_file).unwrap();
        db.close().unwrap();

        std::fs::write(dst_dir.0.join("existing.txt"), b"keep").unwrap();
        let err = Db::import_backup_into_checked(&bkp_file, &dst_dir.0, false)
            .err()
            .expect("expected refusal on non-empty target dir");
        assert!(err.to_string().contains("refusing to overwrite"));
        assert!(dst_dir.0.join("existing.txt").exists());

        let mut imported = Db::import_backup_into_checked(&bkp_file, &dst_dir.0, true).unwrap();
        assert_eq!(imported.stats().docs, 1);
        imported.close().unwrap();
    }

    #[test]
    fn cold_zero_promote_docs_without_uri_and_nan_unique_key() {
        let dir = Temp(std::env::temp_dir().join(format!(
            "lin-p10-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        let opts = crate::OpenOpts {
            cold: true,
            sync: crate::persist::SyncMode::Normal,
        };
        let mut db = Db::open_with(&dir.0, opts).unwrap();
        let docs_seed: Vec<String> = (0..40)
            .map(|i| format!(r#"{{ id: "d{i:02}", uri: "lin://d{i:02}", title: "Doc {i}" }}"#))
            .collect();
        db.run(&format!("insert docs [{}]", docs_seed.join(","))).unwrap();

        db.run("col floats { id: text, v: f64 }\nindex floats unique [v]")
            .unwrap();
        let floats_seed: Vec<String> = (0..40)
            .map(|i| format!(r#"{{ id: "f{i:02}", v: {}.5 }}"#, i))
            .collect();
        db.run(&format!("insert floats [{}]", floats_seed.join(",")))
            .unwrap();
        db.checkpoint().unwrap();
        db.close().unwrap();

        let mut db = Db::open_with(&dir.0, opts).unwrap();
        assert!(db.store.is_disk_backed("docs"));
        assert!(db.store.is_disk_backed("floats"));

        // Multiple docs without `uri` do not conflict and stay on the cold overlay!
        db.run(r#"insert docs { id: "no_uri_1", title: "A" }"#).unwrap();
        db.run(r#"insert docs { id: "no_uri_2", title: "B" }"#).unwrap();
        assert!(
            db.store.is_disk_backed("docs") && !db.store.disk_rows_materialized("docs"),
            "docs without uri must remain cold without promotion"
        );
        assert_eq!(db.run("docs | count").unwrap().scalar().unwrap(), Cell::Int(42));

        // First NaN in unique float key succeeds on cold overlay; second NaN conflicts!
        let mut nan_batch = crate::batch::RecordBatch::with_capacity(
            std::sync::Arc::from([String::from("id"), String::from("v")]),
            1,
        );
        nan_batch.cols[0].push(Cell::from_text("nan1"));
        nan_batch.cols[1].push(Cell::Float(f64::NAN));
        db.insert_batch("floats", &nan_batch).unwrap();
        assert!(
            db.store.is_disk_backed("floats") && !db.store.disk_rows_materialized("floats"),
            "NaN unique-key insert must remain cold without promotion"
        );

        let mut nan_dup = crate::batch::RecordBatch::with_capacity(
            std::sync::Arc::from([String::from("id"), String::from("v")]),
            1,
        );
        nan_dup.cols[0].push(Cell::from_text("nan2"));
        nan_dup.cols[1].push(Cell::Float(f64::NAN));
        let err = db.insert_batch("floats", &nan_dup).unwrap_err();
        assert!(err.to_string().contains("unique index"));
    }

    #[test]
    fn cold_lazy_cursors_non_id_join_and_multihop_without_hydration() {
        let dir = Temp(std::env::temp_dir().join(format!(
            "lin-p32-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        let opts = crate::OpenOpts {
            cold: true,
            sync: crate::persist::SyncMode::Normal,
        };
        let mut cold_db = Db::open_with(&dir.0, opts).unwrap();
        let mut hot_db = Db::empty();

        let schema = "col accounts { id: text, code: text, tier: text }\ncol events { id: text, acct_code: text, amt: i64 }\nfk events.acct_code -> accounts.code";
        cold_db.run(schema).unwrap();
        hot_db.run(schema).unwrap();

        let accts: Vec<String> = (0..40)
            .map(|i| format!(r#"{{ id: "a{i:02}", code: "C{i:02}", tier: "t{}" }}"#, i % 3))
            .collect();
        let evs: Vec<String> = (0..40)
            .map(|i| format!(r#"{{ id: "e{i:02}", acct_code: "C{:02}", amt: {} }}"#, i % 20, i * 5))
            .collect();
        let docs: Vec<String> = (4..40)
            .map(|i| format!(r#"{{ id: "d{i:02}", uri: "lin://d{i:02}", title: "Node {i}" }}"#))
            .collect();
        let data = format!(
            "insert accounts [{}]\ninsert events [{}]\ninsert docs {{ id: \"d00\", uri: \"lin://d00\", title: \"Node 0\" }} with edge wikilink -> \"d01\"\ninsert docs {{ id: \"d01\", uri: \"lin://d01\", title: \"Node 1\" }} with edge wikilink -> \"d02\"\ninsert docs {{ id: \"d02\", uri: \"lin://d02\", title: \"Node 2\" }} with edge wikilink -> \"d03\"\ninsert docs {{ id: \"d03\", uri: \"lin://d03\", title: \"Node 3\" }}\ninsert docs [{}]",
            accts.join(","),
            evs.join(","),
            docs.join(",")
        );
        cold_db.run(&data).unwrap();
        hot_db.run(&data).unwrap();
        cold_db.checkpoint().unwrap();
        cold_db.close().unwrap();

        let mut cold_db = Db::open_with(&dir.0, opts).unwrap();
        cold_db
            .run("fk events.acct_code -> accounts.code\ncol small_ev { id: text, acct_code: text }\nfk small_ev.acct_code -> accounts.code\ninsert small_ev [{ id: \"s1\", acct_code: \"C03\" }, { id: \"s2\", acct_code: \"C07\" }]")
            .unwrap();
        assert!(cold_db.store.is_disk_backed("accounts"));
        assert!(cold_db.store.is_disk_backed("events"));
        assert!(cold_db.store.is_disk_backed("docs"));

        // 1a. Cold DiskJoinCursor on non-`id` FK (`events.acct_code -> accounts.code`).
        let crate::Stmt::Query(q_join) = crate::parse(
            "events | join accounts on acct_code | pick { id, accounts.tier, amt } | take 10",
        )
        .unwrap() else {
            panic!("expected query");
        };
        let cur_join = cold_db.cursor(&q_join).unwrap();
        assert!(cur_join.is_lazy(), "expected lazy DiskJoinCursor for non-id FK");
        let cur_rows: Vec<_> = cur_join.collect::<Result<Vec<_>, _>>().unwrap();
        let hot_rows = hot_db
            .run("events | join accounts on acct_code | pick { id, accounts.tier, amt } | take 10")
            .unwrap()
            .into_rows();
        assert_eq!(cur_rows, hot_rows);
        assert!(!cold_db.store.disk_rows_materialized("events"));
        assert!(!cold_db.store.disk_rows_materialized("accounts"));

        // 1b. Hot-left -> cold-right LazyJoinCursor on non-`id` FK (`small_ev.acct_code -> accounts.code`).
        let crate::Stmt::Query(q_lazy_join) = crate::parse(
            "small_ev | join accounts on acct_code | pick { id, accounts.tier } | take 10",
        )
        .unwrap() else {
            panic!("expected query");
        };
        let cur_lazy_join = cold_db.cursor(&q_lazy_join).unwrap();
        assert!(
            cur_lazy_join.is_lazy(),
            "expected lazy LazyJoinCursor for hot-left cold-right non-id FK"
        );
        let lazy_rows: Vec<_> = cur_lazy_join.collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(lazy_rows.len(), 2);
        assert!(!cold_db.store.disk_rows_materialized("accounts"));

        // 2. Cold multi-hop cursor (`hop wikilink depth=3`) without hydrating `docs`.
        let crate::Stmt::Query(q_hop) = crate::parse(
            r#"docs | id == "d00" | hop wikilink depth=3 | pick { id, title } | take all"#,
        )
        .unwrap() else {
            panic!("expected query");
        };
        let cur_hop = cold_db.cursor(&q_hop).unwrap();
        assert!(cur_hop.is_lazy(), "expected lazy DiskIdsCursor for depth-3 hop");
        let hop_rows: Vec<_> = cur_hop.collect::<Result<Vec<_>, _>>().unwrap();
        let hot_hop = hot_db
            .run(r#"docs | id == "d00" | hop wikilink depth=3 | pick { id, title } | take all"#)
            .unwrap()
            .into_rows();
        assert_eq!(hop_rows, hot_hop);
        assert_eq!(hop_rows.len(), 3);
        assert!(!cold_db.store.disk_rows_materialized("docs"));
    }

    #[test]
    fn window_columnar_group_filter_without_and_sparse_rows() {
        let mut db = Db::empty();
        db.run("col items { id: text, grp: text, score: i64, note: text }")
            .unwrap();
        // Mix rows with `note` present and rows with `note` omitted!
        db.run(
            r#"insert items [
                { id: "i1", grp: "a", score: 30, note: "present" },
                { id: "i2", grp: "a", score: 10 },
                { id: "i3", grp: "b", score: 20, note: "also" },
                { id: "i4", grp: "b", score: 40 }
            ]"#,
        )
        .unwrap();

        // 1. Unprojected window over mixed-sparsity rows preserves absent vs present keys!
        let rows = db
            .run("items | window rank by grp sort score desc as rnk | sort id asc | take all")
            .unwrap()
            .into_rows();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].get("note").and_then(Cell::text), Some("present"));
        assert!(!rows[1].contains_key("note"), "absent field must stay absent");
        assert_eq!(rows[0].get("rnk"), Some(&Cell::Int(1)));
        assert_eq!(rows[1].get("rnk"), Some(&Cell::Int(2)));

        // 2. Post-window filter + group (`count by`) + project via `run_batch` and `run`!
        let q_grp = "items | window rank by grp sort score desc as rnk | rnk <= 1 | count by grp";
        let prep = db.prepare(q_grp).unwrap();
        let batch_rows = prep.run_batch(&mut db).unwrap().to_rows();
        let eager_rows = db.run(q_grp).unwrap().into_rows();
        assert_eq!(batch_rows, eager_rows);
        assert_eq!(batch_rows.len(), 2);

        // 3. `without` after window.
        let q_wo =
            "items | window row_number by grp sort score desc as rn | without { note } | sort id asc | take all";
        let wo_rows = db.run(q_wo).unwrap().into_rows();
        assert!(wo_rows.iter().all(|r| !r.contains_key("note") && r.contains_key("rn")));
    }

    #[test]
    fn persisted_insert_batch_registers_direct_segment_and_exports_wal() {
        let primary_dir = Temp(std::env::temp_dir().join(format!(
            "lin-p41-pri-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        let follower_dir = Temp(std::env::temp_dir().join(format!(
            "lin-p41-fol-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        let opts = crate::OpenOpts {
            cold: true,
            sync: crate::persist::SyncMode::Normal,
        };
        let mut primary = Db::open_with(&primary_dir.0, opts).unwrap();
        let mut follower = Db::open_follower_with(&follower_dir.0, opts).unwrap();

        primary
            .run("col telemetry { id: text, host: text, latency: i64 }")
            .unwrap();
        let n = 5_000usize;
        let mut batch = crate::batch::RecordBatch::with_capacity(
            std::sync::Arc::from([
                String::from("id"),
                String::from("host"),
                String::from("latency"),
            ]),
            n,
        );
        for i in 0..n {
            batch.cols[0].push(Cell::from_text(format!("t{i:05}")));
            batch.cols[1].push(Cell::from_text(format!("h{}", i % 8)));
            batch.cols[2].push(Cell::Int(i as i64));
        }
        let done = primary.insert_batch("telemetry", &batch).unwrap();
        assert_eq!(done.n, n);
        assert!(
            primary.store.is_disk_backed("telemetry")
                && !primary.store.disk_rows_materialized("telemetry"),
            "5k insert_batch under Persist WAL should write an immutable cold segment directly"
        );
        assert!(
            primary
                .store
                .disk_engine_read()
                .is_some_and(|e| !e.table_has_dirty_overlay("telemetry")),
            "direct segment insert_batch must bypass the memtable overlay"
        );

        // Export WAL to an isolated follower directory (no shared `segments/` folder!).
        let wal = primary.export_wal_since(0).unwrap();
        follower.apply_wal(&wal).unwrap();
        assert_eq!(
            follower.run("telemetry | count").unwrap().scalar().unwrap(),
            Cell::Int(n as i64)
        );

        // Reopen primary without checkpointing to verify `Pack::RegisterSegment` WAL replay!
        drop(primary);
        let mut reopened = Db::open_with(&primary_dir.0, opts).unwrap();
        assert!(reopened.store.is_disk_backed("telemetry"));
        assert_eq!(
            reopened.run("telemetry | count").unwrap().scalar().unwrap(),
            Cell::Int(n as i64)
        );
        reopened.close().unwrap();
        follower.close().unwrap();
    }
}
