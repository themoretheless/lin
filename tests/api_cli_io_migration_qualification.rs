//! End-to-end qualification suite for Wave 2C (`D37`–`D39`, `D46`–`D48`):
//! - Bounded streaming JSONL & RFC-4180 CSV import/export, schema inference, mapping, reject sink, cancellation, vector dim validation, columnar IPC wire roundtrip
//! - Catalog introspection, schema diffing, compatibility risk labels, dry-run checks, and transactional `apply_migration` with atomic rollback
//! - `DbBuilder`, `StorageCapabilities`, `QueryCancelToken`, `QueryOptions`, `PreparedSchemaInfo`, `BorrowedText`, `map_batch`, `into_typed_iter`, `impl_newtype_cell!`, `impl_str_enum_cell!`, generic `#[derive(LinRow)]`
//! - CLI subprocess qualification (`version --capabilities`, `run --format jsonl|json|table`, `--json-errors`, `import`/`export` stdin/stdout, `inspect wal|storage|indexes|locks`, `health`, `readiness`, `backup verify`, `config effective`, `completions zsh`, multiline `repl`)

use std::io::{BufReader, Cursor, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use lin::catalog::Type;
use lin::error::ErrorCode;
use lin::exec::{
    Db, DbBuilder, PreparedSchemaInfo, QueryCancelToken, QueryOptions, Quotas, SyncMode,
};
use lin::io_formats::{
    ImportLimits, ImportSchemaMapping, export_columnar_ipc_batches, export_csv_writer,
    export_jsonl_writer, import_columnar_ipc_batches, import_csv_reader, import_jsonl_reader,
    infer_csv_schema, infer_jsonl_schema,
};
use lin::migration::{
    MigrationPlan, MigrationRiskLabel, SchemaChangeKind, apply_migration, diff_catalogs,
    dry_run_migration, inspect_catalog, inspect_db_catalog,
};
use lin::row::{BorrowedText, FromCell, ToCell, cell_get};
use lin::{Cell, impl_newtype_cell, impl_str_enum_cell};

static TEST_SEQ: AtomicUsize = AtomicUsize::new(1);

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        let id = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "lin-w2c-{prefix}-{}-{id}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------------------
// 1. Streaming JSONL & RFC-4180 CSV Import/Export + Columnar IPC (`D47`)
// ---------------------------------------------------------------------------

#[test]
fn rfc4180_csv_embedded_newlines_escaped_quotes_and_null_distinction() {
    let csv_input = concat!(
        "raw_id,full_name,bio,score,note\r\n",
        "c1,\"Ada, \"\"Lovelace\"\"\",\"line one\nline two\r\nline three\",98.5,\"\"\r\n",
        "c2,Alan Turing,,100,NULL\r\n",
        "c3,Grace Hopper,\"quoted \"\"NULL\"\"\",42.0,plain\n"
    );

    let mut mapping = ImportSchemaMapping::default();
    mapping
        .field_renames
        .insert("raw_id".to_string(), "id".to_string());
    mapping
        .field_renames
        .insert("full_name".to_string(), "name".to_string());
    mapping.null_sentinel = "NULL".to_string();
    mapping.empty_csv_as_null = true;
    mapping
        .defaults
        .insert("tier".to_string(), Cell::from_text("gold"));

    let inferred = infer_csv_schema(
        Cursor::new(csv_input.as_bytes()),
        10,
        &ImportLimits::default(),
        &mapping,
    )
    .expect("infer csv schema");
    assert_eq!(inferred.sampled_rows, 3);
    assert_eq!(inferred.fields.get("score"), Some(&Type::F64));
    assert!(inferred.nullable_fields.contains("bio"));
    assert!(inferred.nullable_fields.contains("note"));

    let mut db = Db::empty();
    let mut progress_calls = 0usize;
    let receipt = import_csv_reader(
        &mut db,
        "people",
        &mut Cursor::new(csv_input.as_bytes()),
        &ImportLimits {
            batch_size: 2,
            ..ImportLimits::default()
        },
        &mapping,
        None,
        &mut |_bytes, _rows| progress_calls += 1,
        &|| false,
    )
    .expect("import csv");

    assert_eq!(receipt.imported_rows, 3);
    assert_eq!(receipt.rejected_rows, 0);
    assert_eq!(receipt.batches_committed, 2);
    assert!(progress_calls >= 2);
    assert_ne!(receipt.source_sha256_fnv, 0);

    let h = db.run("people | sort id | take all").unwrap();
    assert_eq!(h.rows.len(), 3);

    // c1: quoted "" on `note` stays empty string "", NOT null! Embedded \n and \r\n preserved!
    let r1 = &h.rows[0];
    assert_eq!(r1.borrow_str("id"), Some("c1"));
    assert_eq!(r1.borrow_str("name"), Some("Ada, \"Lovelace\""));
    assert_eq!(
        r1.borrow_str("bio"),
        Some("line one\nline two\r\nline three")
    );
    assert_eq!(r1.borrow_str("note"), Some(""));
    assert_eq!(r1.borrow_str("tier"), Some("gold"));

    // c2: unquoted empty `,,` on `bio` and unquoted `NULL` sentinel on `note` become absent/null!
    let r2 = &h.rows[1];
    assert_eq!(r2.borrow_str("id"), Some("c2"));
    assert!(r2.get("bio").is_none());
    assert!(r2.get("note").is_none());

    // c3: quoted `"quoted ""NULL"""` is preserved literally.
    let r3 = &h.rows[2];
    assert_eq!(r3.borrow_str("bio"), Some("quoted \"NULL\""));
    assert_eq!(r3.borrow_str("note"), Some("plain"));

    // Deterministic CSV export roundtrip preserves embedded newlines, escaped quotes, and `""` vs `,,`!
    let mut exported_csv = Vec::new();
    let exported_n =
        export_csv_writer(&mut db, "people | sort id | take all", &mut exported_csv).unwrap();
    assert_eq!(exported_n, 3);

    let mut db2 = Db::empty();
    let receipt2 = import_csv_reader(
        &mut db2,
        "people_rt",
        &mut Cursor::new(&exported_csv),
        &ImportLimits::default(),
        &ImportSchemaMapping::default(),
        None,
        &mut |_, _| {},
        &|| false,
    )
    .unwrap();
    assert_eq!(receipt2.imported_rows, 3);
    let rt_rows = db2.run("people_rt | sort id | take all").unwrap().rows.clone();
    assert_eq!(rt_rows[0].borrow_str("bio"), Some("line one\nline two\r\nline three"));
    assert_eq!(rt_rows[0].borrow_str("note"), Some(""));
    assert!(rt_rows[1].get("bio").is_none());
}

#[test]
fn jsonl_schema_inference_reject_mode_row_errors_cancellation_and_vec_dim() {
    let sample_jsonl = concat!(
        r#"{"id":"j1","count":10,"ratio":1.5,"active":true,"embedding":[0.25,0.75]}"#, "\n",
        r#"{"id":"j2","count":20,"ratio":null,"active":false,"embedding":[0.5,0.5],"extra":"hi"}"#, "\n",
    );
    let schema = infer_jsonl_schema(
        Cursor::new(sample_jsonl.as_bytes()),
        10,
        &ImportLimits::default(),
    )
    .unwrap();
    assert_eq!(schema.sampled_rows, 2);
    assert_eq!(schema.fields.get("count"), Some(&Type::I64));
    assert_eq!(schema.fields.get("ratio"), Some(&Type::F64));
    assert_eq!(schema.fields.get("active"), Some(&Type::Bool));
    assert_eq!(schema.fields.get("embedding"), Some(&Type::Vec));
    assert!(schema.nullable_fields.contains("ratio"));
    assert!(schema.nullable_fields.contains("extra"));

    // Reject-file mode captures malformed JSON + wrong vector dim + type error without aborting good rows
    let mixed_jsonl = concat!(
        r#"{"id":"ok1","count":1,"embedding":[1.0,0.0]}"#, "\n",
        r#"{"id":"bad_json", broken"#, "\n",
        r#"{"id":"bad_dim","count":2,"embedding":[1.0,0.0,0.0]}"#, "\n",
        r#"{"id":"ok2","count":3,"embedding":[0.0,1.0]}"#, "\n",
    );
    let mut db = Db::empty();
    let mut rejects: Vec<u8> = Vec::new();
    let limits = ImportLimits {
        batch_size: 2,
        expected_vector_dim: Some(2),
        ..ImportLimits::default()
    };
    let receipt = import_jsonl_reader(
        &mut db,
        "items",
        &mut Cursor::new(mixed_jsonl.as_bytes()),
        &limits,
        &ImportSchemaMapping::default(),
        Some(&mut rejects),
        &mut |_, _| {},
        &|| false,
    )
    .unwrap();
    assert_eq!(receipt.imported_rows, 2);
    assert_eq!(receipt.rejected_rows, 2);
    assert_eq!(receipt.errors.len(), 2);
    assert_eq!(receipt.errors[0].row_number, 2);
    assert_eq!(receipt.errors[1].row_number, 3);
    let reject_str = String::from_utf8(rejects).unwrap();
    assert!(reject_str.contains("bad_json"));
    assert!(reject_str.contains("bad_dim"));

    // Strict mode (reject_writer = None) reports exact row number and IncompatibleModel on dim mismatch
    let err = import_jsonl_reader(
        &mut db,
        "items",
        &mut Cursor::new(mixed_jsonl.as_bytes()),
        &limits,
        &ImportSchemaMapping {
            start_row_offset: 2,
            ..ImportSchemaMapping::default()
        },
        None,
        &mut |_, _| {},
        &|| false,
    )
    .unwrap_err();
    assert_eq!(err.code(), ErrorCode::IncompatibleModel);
    assert!(err.message.contains("row 3"), "{}", err.message);

    // Cooperative cancellation mid-stream commits completed batches and sets `cancelled = true`
    let many_jsonl: String = (0..10)
        .map(|i| format!(r#"{{"id":"m{i}","count":{i}}}"#))
        .collect::<Vec<_>>()
        .join("\n");
    let seen_batches = AtomicUsize::new(0);
    let mut db_cancel = Db::empty();
    let cancel_flag = std::sync::atomic::AtomicBool::new(false);
    let cancel_receipt = import_jsonl_reader(
        &mut db_cancel,
        "streamed",
        &mut Cursor::new(many_jsonl.as_bytes()),
        &ImportLimits {
            batch_size: 3,
            ..ImportLimits::default()
        },
        &ImportSchemaMapping::default(),
        None,
        &mut |_bytes, _rows| {
            if seen_batches.fetch_add(1, Ordering::SeqCst) + 1 >= 2 {
                cancel_flag.store(true, Ordering::SeqCst);
            }
        },
        &|| cancel_flag.load(Ordering::SeqCst),
    )
    .unwrap();
    assert!(cancel_receipt.cancelled);
    assert_eq!(cancel_receipt.imported_rows, 6);
    assert_eq!(cancel_receipt.batches_committed, 2);

    // JSONL export deterministic ordering
    let mut exported_jsonl = Vec::new();
    let n_exp = export_jsonl_writer(&mut db_cancel, "streamed | take all", &mut exported_jsonl).unwrap();
    assert_eq!(n_exp, 6);
    let lines: Vec<&str> = std::str::from_utf8(&exported_jsonl).unwrap().lines().collect();
    assert_eq!(lines.len(), 6);
    assert!(lines[0].starts_with(r#"{"count":0,"id":"m0"}"#), "{}", lines[0]);

    // Columnar wire IPC roundtrip (`export_columnar_ipc_batches` / `import_columnar_ipc_batches`)
    let batch1 = db_cancel.run_batch("streamed | sort id | take 3").unwrap();
    let batch2 = db_cancel.run_batch("streamed | sort id | skip 3 | take 3").unwrap();
    let mut ipc_bytes = Vec::new();
    let total_ipc_rows = export_columnar_ipc_batches(&[batch1.clone(), batch2.clone()], &mut ipc_bytes).unwrap();
    assert_eq!(total_ipc_rows, 6);
    let decoded_batches =
        import_columnar_ipc_batches(&mut BufReader::new(Cursor::new(&ipc_bytes)), &ImportLimits::default())
            .unwrap();
    assert_eq!(decoded_batches.len(), 2);
    assert_eq!(decoded_batches[0], batch1);
    assert_eq!(decoded_batches[1], batch2);
}

// ---------------------------------------------------------------------------
// 2. Catalog Introspection, Schema Diffing, & Transactional Migrations (`D46`)
// ---------------------------------------------------------------------------

#[test]
fn catalog_introspection_diff_and_transactional_migrations_with_rollback() {
    let tmp = TempDir::new("migration");
    let data_dir = tmp.path().join("db");

    let mut db = Db::open(&data_dir).unwrap();
    db.run("col accounts { id: text, handle: text, balance_str: text, legacy: text }\nindex accounts [handle]").unwrap();
    db.run(r#"insert accounts [
        { id: "a1", handle: "ada", balance_str: "1200", legacy: "old1" },
        { id: "a2", handle: "alan", balance_str: "3400", legacy: "old2" }
    ]"#).unwrap();

    let intro_before = inspect_db_catalog(&db);
    assert!(intro_before.collections.contains_key("accounts"));
    assert!(intro_before.scalar_indexes.contains_key("accounts[handle]"));
    assert!(intro_before.fts_fields.get("accounts").unwrap().contains(&"handle".to_string()));
    assert_eq!(intro_before.embedder_id, db.catalog.embed_id);

    let base_cat = db.catalog.clone();

    // Dry-run a DropFieldChecked on indexed field `handle` -> reports dependency conflict without mutating!
    let bad_drop_plan = vec![MigrationPlan::DropFieldChecked {
        collection: "accounts".to_string(),
        field: "handle".to_string(),
    }];
    let bad_dry = dry_run_migration(&db, &bad_drop_plan).unwrap();
    assert!(!bad_dry.valid);
    assert!(!bad_dry.dependency_conflicts.is_empty());

    // Dry-run nullable-to-required violation detection
    db.run(r#"insert accounts { id: "a3", handle: "grace", balance_str: "900" }"#).unwrap();
    let req_violation_dry = dry_run_migration(
        &db,
        &[MigrationPlan::ConvertFieldType {
            collection: "accounts".to_string(),
            field: "legacy".to_string(),
            to_type: Type::Text,
            required: true,
        }],
    )
    .unwrap();
    assert!(!req_violation_dry.valid);
    assert_eq!(req_violation_dry.nullable_to_required_violations.len(), 1);

    // Valid multi-step migration plan: AddFieldWithDefault -> DropFieldChecked -> RenameField -> ConvertFieldType -> RenameCollection
    let valid_plan = vec![
        MigrationPlan::AddFieldWithDefault {
            collection: "accounts".to_string(),
            field: "active".to_string(),
            ty: Type::Bool,
            default: Cell::Bool(true),
        },
        MigrationPlan::DropFieldChecked {
            collection: "accounts".to_string(),
            field: "legacy".to_string(),
        },
        MigrationPlan::RenameField {
            collection: "accounts".to_string(),
            from: "handle".to_string(),
            to: "username".to_string(),
        },
        MigrationPlan::ConvertFieldType {
            collection: "accounts".to_string(),
            field: "balance_str".to_string(),
            to_type: Type::I64,
            required: true,
        },
        MigrationPlan::RenameCollection {
            from: "accounts".to_string(),
            to: "members".to_string(),
        },
    ];

    let ok_dry = dry_run_migration(&db, &valid_plan).unwrap();
    assert!(ok_dry.valid, "{ok_dry:?}");
    assert!(ok_dry.affected_rows >= 3);
    assert!(ok_dry.rebuilt_indexes.contains(&"members[username]".to_string()));

    let mut progress_steps = Vec::new();
    let mig_receipt = apply_migration(&mut db, &valid_plan, &mut |done, total| {
        progress_steps.push((done, total));
    })
    .unwrap();
    assert_eq!(mig_receipt.steps_applied, 5);
    assert_eq!(progress_steps.last(), Some(&(5, 5)));
    assert_ne!(mig_receipt.previous_catalog_hash, mig_receipt.new_catalog_hash);

    // Diff old catalog vs migrated catalog
    let diff = diff_catalogs(&base_cat, &db.catalog);
    assert!(diff.irreversible);
    assert!(!diff.backward_compatible);
    assert!(!diff.downgrade_feasible);
    assert!(
        diff.changes
            .iter()
            .any(|(k, _, r)| *k == SchemaChangeKind::RemoveCollection
                && *r == MigrationRiskLabel::IrreversibleDataLoss)
    );
    assert!(
        diff.changes
            .iter()
            .any(|(k, _, r)| *k == SchemaChangeKind::AddCollection
                && *r == MigrationRiskLabel::BackwardCompatible)
    );

    // Check BackwardCompatible + DowngradeFeasible diff when only adding a collection + index
    let mut additive_cat = base_cat.clone();
    additive_cat
        .apply_decl(&lin::ast::Decl::Index {
            collection: "accounts".to_string(),
            unique: false,
            fields: vec!["legacy".to_string()],
            pred: None,
        })
        .unwrap();
    let additive_diff = diff_catalogs(&base_cat, &additive_cat);
    assert!(additive_diff.backward_compatible);
    assert!(!additive_diff.irreversible);
    assert!(additive_diff.downgrade_feasible);
    assert_eq!(additive_diff.changes[0].2, MigrationRiskLabel::RequiresRebuild);

    // Test atomic rollback when step 2 fails conversion!
    let gen_before_fail = db.r#gen();
    let hash_before_fail = inspect_catalog(&db.catalog).schema_hash;
    db.run(r#"insert members { id: "m_bad", username: "notnum", balance_str: 50, active: true }"#)
        .unwrap();
    let pre_fail_gen = db.r#gen();
    let failing_plan = vec![
        MigrationPlan::AddFieldWithDefault {
            collection: "members".to_string(),
            field: "temp_col".to_string(),
            ty: Type::I64,
            default: Cell::Int(99),
        },
        MigrationPlan::ConvertFieldType {
            collection: "members".to_string(),
            field: "username".to_string(),
            to_type: Type::I64,
            required: true,
        },
    ];
    let err = apply_migration(&mut db, &failing_plan, &mut |_, _| {}).unwrap_err();
    assert!(err.message.contains("cannot convert text"));
    assert_eq!(db.r#gen(), pre_fail_gen);
    assert_eq!(inspect_catalog(&db.catalog).schema_hash, hash_before_fail);
    assert!(db.catalog.collection("members").unwrap().field("temp_col").is_none());
    assert!(gen_before_fail < pre_fail_gen);

    // Close and reopen durable DB: migrated `members` schema, index, and converted i64 values survive!
    db.close().unwrap();
    let mut reopened = Db::open(&data_dir).unwrap();
    let rows = reopened
        .run("members | balance_str >= 1200 | sort balance_str | take all")
        .unwrap()
        .rows
        .clone();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].borrow_str("username"), Some("ada"));
    assert_eq!(rows[0].get("balance_str"), Some(&Cell::Int(1200)));
    assert_eq!(rows[0].get("active"), Some(&Cell::Bool(true)));
    reopened.close().unwrap();
}

// ---------------------------------------------------------------------------
// 3. DbBuilder, QueryOptions, PreparedSchemaInfo, Row Macros & Derive (`D37`, `D38`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountId(pub String);
impl_newtype_cell!(AccountId, String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocStatus {
    Draft,
    Published,
    Archived,
}
impl_str_enum_cell!(DocStatus {
    Draft => "draft",
    Published => "published",
    Archived => "archived",
});

#[cfg(feature = "derive")]
mod derive_checks {
    use super::*;
    use lin::LinRow;
    use lin::row::{FromRow, map_batch};
    use lin::{RecordBatch, Row};

    pub use lin as custom_lin_alias;

    #[derive(Debug, PartialEq, LinRow)]
    #[lin(collection = "items", crate = "custom_lin_alias")]
    pub struct GenericItem<'a, T> {
        pub id: AccountId,
        #[lin(rename = "state")]
        pub status: DocStatus,
        pub metric: T,
        #[lin(skip)]
        pub transient: usize,
        #[lin(skip)]
        pub _lt: core::marker::PhantomData<&'a ()>,
    }

    #[test]
    pub fn generic_lin_row_custom_crate_newtype_enum_and_batch_mapping() {
        let mut db = Db::empty();
        db.run("col items { id: text, state: text, metric: i64 }").unwrap();
        db.run(r#"insert items [
            { id: "i1", state: "draft", metric: 10 },
            { id: "i2", state: "published", metric: 25 }
        ]"#)
        .unwrap();

        assert_eq!(GenericItem::<i64>::COLLECTION, Some("items"));
        assert_eq!(GenericItem::<i64>::COLUMNS, &["id", "state", "metric"]);

        // Test `RecordBatch::to_typed_rows` & `map_batch`
        let batch: RecordBatch = db.run_batch("items | sort id | take all").unwrap();
        let typed_from_batch: Vec<GenericItem<'_, i64>> = batch.to_typed_rows().unwrap();
        let typed_via_fn: Vec<GenericItem<'_, i64>> = map_batch(&batch).unwrap();
        assert_eq!(typed_from_batch, typed_via_fn);
        assert_eq!(typed_from_batch.len(), 2);
        assert_eq!(typed_from_batch[0].id, AccountId("i1".to_string()));
        assert_eq!(typed_from_batch[0].status, DocStatus::Draft);
        assert_eq!(typed_from_batch[0].metric, 10);
        assert_eq!(typed_from_batch[0].transient, 0);

        // Test streamed `QueryCursor::into_typed_iter`
        let StmtQuery(q) = parse_query("items | sort id | take all");
        let streamed: Vec<GenericItem<'_, i64>> = db
            .cursor(&q)
            .unwrap()
            .into_typed_iter::<GenericItem<'_, i64>>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(streamed, typed_from_batch);

        // Test field-provenance enrichment when `FromRow` fails (`"field `state`: ..."`)
        let mut bad_row = Row::new();
        bad_row.insert("id", AccountId("i3".into()).to_cell());
        bad_row.insert("state", Cell::from_text("invalid_variant"));
        bad_row.insert("metric", Cell::Int(5));
        let err = GenericItem::<i64>::from_row(&bad_row).unwrap_err();
        assert!(err.message.starts_with("field `state`:"), "{}", err.message);
        assert_eq!(err.field(), Some("state"));
    }

    struct StmtQuery(lin::Query);
    fn parse_query(s: &str) -> StmtQuery {
        match lin::parse(s).unwrap() {
            lin::Stmt::Query(q) => StmtQuery(q),
            other => panic!("expected query, got {other:?}"),
        }
    }
}

#[test]
fn db_builder_capabilities_query_options_prepared_schema_and_borrowed_text() {
    let tmp = TempDir::new("builder");
    let dir = tmp.path().join("store");

    let mut db = DbBuilder::new()
        .data_dir(&dir)
        .quotas(Quotas {
            max_rows: 50_000,
            ..Quotas::default()
        })
        .sync_mode(SyncMode::Normal)
        .cold(true)
        .strict_embed_dim(true)
        .build()
        .unwrap();

    let caps = db.capabilities();
    assert!(caps.durable);
    assert!(!caps.read_only);
    assert!(caps.cold_tier);
    assert!(caps.strict_embed_dim);
    assert_eq!(caps.sync_mode, SyncMode::Normal);
    assert!(!caps.supported_wal_codecs.is_empty());

    db.run("col logs { id: text, msg: text, code: i64 }").unwrap();
    db.run(r#"insert logs [
        { id: "l1", msg: "alpha", code: 200 },
        { id: "l2", msg: "beta", code: 201 },
        { id: "l3", msg: "gamma", code: 500 },
        { id: "l4", msg: "delta", code: 404 }
    ]"#)
    .unwrap();
    db.build_hnsw("docs").unwrap();

    // Prepared::schema_info & Prepared::is_stale_for
    let prep = db.prepare("logs | code >= 200 | { id, code } | sort id").unwrap();
    let info: PreparedSchemaInfo = prep.schema_info(&db);
    assert!(info.is_read_only);
    assert_eq!(info.target_collection.as_deref(), Some("logs"));
    assert_eq!(
        info.output_fields,
        vec![
            ("id".to_string(), "text".to_string()),
            ("code".to_string(), "i64".to_string()),
        ]
    );
    assert!(!prep.is_stale_for(&db));

    db.run("index logs [code]").unwrap();
    assert!(prep.is_stale_for(&db));

    // QueryOptions: max_rows + exact_vector_search
    let (h, stats) = db
        .execute_with_options(
            "logs | sort id | take all",
            &QueryOptions {
                max_rows: Some(2),
                exact_vector_search: true,
                ..QueryOptions::default()
            },
        )
        .unwrap();
    assert_eq!(h.rows.len(), 2);
    assert!(stats.truncated_by_max_rows);
    assert!(stats.used_exact_vector_search);
    assert_eq!(stats.rows_returned, 2);

    // BorrowedText & Row::borrow_str + cell_get provenance
    let r0 = &h.rows[0];
    assert_eq!(r0.borrow_str("id"), Some("l1"));
    let bt: BorrowedText<'_> = r0.borrow_text("msg").unwrap();
    assert_eq!(bt.as_str(), "alpha");
    let prov_err = cell_get::<i64>(r0, "msg").unwrap_err();
    assert!(prov_err.message.starts_with("field `msg`:"), "{}", prov_err.message);
    assert_eq!(AccountId("acc-1".into()).to_cell(), Cell::from_text("acc-1"));
    assert_eq!(AccountId::from_cell(&Cell::from_text("acc-1")).unwrap().0, "acc-1");
    assert_eq!(DocStatus::Published.to_cell(), Cell::from_text("published"));
    assert_eq!(DocStatus::from_cell(&Cell::from_text("archived")).unwrap(), DocStatus::Archived);

    // QueryCancelToken cancellation
    let token = QueryCancelToken::new();
    token.cancel();
    let cancel_err = db
        .execute_with_options(
            "logs | take all",
            &QueryOptions {
                cancel_token: Some(token),
                ..QueryOptions::default()
            },
        )
        .unwrap_err();
    assert_eq!(cancel_err.code(), ErrorCode::Canceled);

    // Deadline expiration
    let expired = Instant::now() - Duration::from_millis(5);
    let dl_err = db
        .execute_with_options(
            "logs | take all",
            &QueryOptions {
                deadline: Some(expired),
                ..QueryOptions::default()
            },
        )
        .unwrap_err();
    assert_eq!(dl_err.code(), ErrorCode::DeadlineExceeded);

    db.close().unwrap();

    // Reopen via DbBuilder with `read_only(true)` — reads succeed, writes rejected!
    let mut ro_db = DbBuilder::new()
        .data_dir(&dir)
        .read_only(true)
        .build()
        .unwrap();
    assert!(ro_db.capabilities().read_only);
    assert_eq!(ro_db.run("logs | count").unwrap().scalar(), Ok(Cell::Int(4)));
    let write_err = ro_db
        .run(r#"insert logs { id: "l5", msg: "nope", code: 403 }"#)
        .unwrap_err();
    assert_eq!(write_err.code(), ErrorCode::PermissionDenied);
}

// ---------------------------------------------------------------------------
// 4. CLI Subprocess Qualification (`D39`, `D48`)
// ---------------------------------------------------------------------------

#[test]
fn cli_capabilities_formats_json_errors_import_export_inspect_health_backup_config_completions_and_repl() {
    let bin = env!("CARGO_BIN_EXE_lin");
    let tmp = TempDir::new("cli");
    let data_dir = tmp.path().join("db");
    let backup_path = tmp.path().join("snap.bak");

    // 1. `lin version --capabilities`
    let ver_out = Command::new(bin)
        .args(["version", "--capabilities"])
        .output()
        .unwrap();
    assert!(ver_out.status.success());
    let ver_json: serde_json::Value = serde_json::from_slice(&ver_out.stdout).unwrap();
    assert_eq!(ver_json["version"], lin::VERSION);
    assert!(ver_json["features"].is_object());
    assert!(ver_json["wal_codecs"].is_array());

    // 2. `lin import` via stdin + `lin export` via stdout + `lin run --format jsonl|json|table`
    let jsonl_payload = b"{\"id\":\"p1\",\"name\":\"Mercury\",\"mass\":3}\n{\"id\":\"p2\",\"name\":\"Venus\",\"mass\":8}\n";
    let mut import_child = Command::new(bin)
        .arg("--data")
        .arg(&data_dir)
        .args(["import", "--collection", "planets", "--format", "jsonl"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    import_child
        .stdin
        .take()
        .unwrap()
        .write_all(jsonl_payload)
        .unwrap();
    let import_out = import_child.wait_with_output().unwrap();
    assert!(
        import_out.status.success(),
        "{}",
        String::from_utf8_lossy(&import_out.stderr)
    );
    let imp_json: serde_json::Value = serde_json::from_slice(&import_out.stdout).unwrap();
    assert_eq!(imp_json["imported_rows"], 2);

    // `run --format jsonl`
    let run_jsonl = Command::new(bin)
        .arg("--data")
        .arg(&data_dir)
        .args(["run", "--format", "jsonl", "planets | sort id | take all"])
        .output()
        .unwrap();
    assert!(run_jsonl.status.success());
    let lines: Vec<serde_json::Value> = std::str::from_utf8(&run_jsonl.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["name"], "Mercury");
    assert_eq!(lines[1]["mass"], 8);

    // `run --format table`
    let run_tbl = Command::new(bin)
        .arg("--data")
        .arg(&data_dir)
        .args(["run", "--format", "table", "planets | sort id | take all"])
        .output()
        .unwrap();
    assert!(run_tbl.status.success());
    let tbl_str = String::from_utf8_lossy(&run_tbl.stdout);
    assert!(tbl_str.contains("Mercury") && tbl_str.contains(" | "), "{tbl_str}");

    // `export` to stdout in CSV format
    let exp_csv = Command::new(bin)
        .arg("--data")
        .arg(&data_dir)
        .args([
            "export",
            "--query",
            "planets | sort id | take all",
            "--format",
            "csv",
        ])
        .output()
        .unwrap();
    assert!(exp_csv.status.success());
    let csv_text = String::from_utf8_lossy(&exp_csv.stdout);
    assert!(csv_text.starts_with("id,mass,name\r\n"), "{csv_text}");
    assert!(csv_text.contains("p1,3,Mercury"));

    // 3. `--json-errors` exit codes (`2` compile/parse, `3` runtime)
    let parse_err_out = Command::new(bin)
        .args(["--json-errors", "run", "planets | ??? invalid syntax"])
        .output()
        .unwrap();
    assert_eq!(parse_err_out.status.code(), Some(2));
    let perr_json: serde_json::Value = serde_json::from_slice(&parse_err_out.stderr).unwrap();
    assert_eq!(perr_json["phase"], "compile");
    assert!(perr_json["code"].as_str().unwrap().starts_with("E_"));

    let runtime_err_out = Command::new(bin)
        .arg("--data")
        .arg(&data_dir)
        .args([
            "--json-errors",
            "run",
            r#"insert planets { id: "p1", name: "Dup", mass: 1 }"#,
        ])
        .output()
        .unwrap();
    assert_eq!(runtime_err_out.status.code(), Some(3));
    let rerr_json: serde_json::Value = serde_json::from_slice(&runtime_err_out.stderr).unwrap();
    assert_eq!(rerr_json["phase"], "runtime");
    assert_eq!(rerr_json["code"], "E_UNIQUE_VIOLATION");

    // 4. `inspect storage|wal|indexes|locks`
    for target in ["storage", "wal", "indexes", "locks"] {
        let insp = Command::new(bin)
            .arg("--data")
            .arg(&data_dir)
            .args(["inspect", target])
            .output()
            .unwrap();
        assert!(
            insp.status.success(),
            "inspect {target} failed: {}",
            String::from_utf8_lossy(&insp.stderr)
        );
        let parsed: serde_json::Value = serde_json::from_slice(&insp.stdout).unwrap();
        assert!(parsed.is_object(), "inspect {target}: {parsed}");
    }

    // 5. `health` & `readiness`
    for probe in ["health", "readiness"] {
        let p_out = Command::new(bin)
            .arg("--data")
            .arg(&data_dir)
            .arg(probe)
            .output()
            .unwrap();
        assert!(p_out.status.success());
        let p_json: serde_json::Value = serde_json::from_slice(&p_out.stdout).unwrap();
        assert_eq!(p_json["status"], "healthy");
        assert_eq!(p_json["ready"], true);
    }

    // 6. `backup export` + `backup verify` (must NOT mutate a non-existent `--data` dir!)
    let exp_bak = Command::new(bin)
        .arg("--data")
        .arg(&data_dir)
        .args(["backup", "export", backup_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(exp_bak.status.success());

    let untouched_dir = tmp.path().join("must_not_be_created");
    let ver_bak = Command::new(bin)
        .arg("--data")
        .arg(&untouched_dir)
        .args(["backup", "verify", backup_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(ver_bak.status.success());
    assert!(!untouched_dir.exists(), "backup verify must not mutate --data");
    let vbak_json: serde_json::Value = serde_json::from_slice(&ver_bak.stdout).unwrap();
    assert_eq!(vbak_json["valid"], true);

    // Corrupt backup returns exit code 4!
    let corrupt_bak = tmp.path().join("corrupt.bak");
    std::fs::write(&corrupt_bak, b"\x03LIN\x04corrupted_payload_LCRC1234").unwrap();
    let bad_bak_out = Command::new(bin)
        .args(["--json-errors", "backup", "verify", corrupt_bak.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(bad_bak_out.status.code(), Some(4));

    // 7. `config effective` with env override
    let cfg_out = Command::new(bin)
        .env("LIN_MAX_ROWS", "77777")
        .env("LIN_COLD", "true")
        .arg("--data")
        .arg(&data_dir)
        .args(["config", "effective"])
        .output()
        .unwrap();
    assert!(cfg_out.status.success());
    let cfg_json: serde_json::Value = serde_json::from_slice(&cfg_out.stdout).unwrap();
    assert_eq!(cfg_json["cold"], true);
    assert_eq!(cfg_json["quotas"]["max_rows"], 77_777);
    assert_eq!(cfg_json["env_overrides"]["LIN_MAX_ROWS"], 77_777);

    // 8. `completions zsh` (and bash, fish)
    for shell in ["zsh", "bash", "fish"] {
        let comp = Command::new(bin).args(["completions", shell]).output().unwrap();
        assert!(comp.status.success());
        let text = String::from_utf8_lossy(&comp.stdout);
        assert!(text.contains("inspect") && text.contains("repl"), "{shell}: {text}");
    }

    // 9. Multiline `repl` reading `;`-terminated statements from stdin
    let repl_script = concat!(
        "insert planets {\n",
        "  id: \"p3\",\n",
        "  name: \"Earth; Blue Marble\",\n",
        "  mass: 10\n",
        "};\n",
        "planets\n  | sort id\n  | take all;\n",
        ":quit\n"
    );
    let mut repl_child = Command::new(bin)
        .arg("--data")
        .arg(&data_dir)
        .args(["repl", "--format", "jsonl"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    repl_child
        .stdin
        .take()
        .unwrap()
        .write_all(repl_script.as_bytes())
        .unwrap();
    let repl_out = repl_child.wait_with_output().unwrap();
    assert!(
        repl_out.status.success(),
        "{}",
        String::from_utf8_lossy(&repl_out.stderr)
    );
    let repl_stdout = String::from_utf8_lossy(&repl_out.stdout);
    assert!(repl_stdout.contains("Earth; Blue Marble"), "{repl_stdout}");
}
