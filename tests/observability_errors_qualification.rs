//! Qualification suite for `D40` (Structured Diagnostics & Errors), `D41` (Runtime Observability & Telemetry),
//! `D35` (Replication Lag Metrics), and `D45` (Security Capabilities, Sandboxed File-Root & UTF-8 Safety).

use std::collections::HashSet;
use std::error::Error as StdError;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Instant;

use lin::error::{
    DiagnosticWarning, Error, ErrorCode, ErrorKind, Phase, WarningChannel, WarningSeverity,
    WarningSuppressionScope, redact_credentials, redact_sensitive_path,
};
use lin::security::{
    CapabilityPolicy, CollectionCapability, DEPENDENCY_PROVENANCE_INVENTORY,
    ExtensionTrustBoundary, RESPONSIBLE_DISCLOSURE_GUIDANCE, ResourceLimitReceipt,
    scan_utf8_safety, validate_c_abi_buffer_descriptor,
};
use lin::telemetry::{
    CollectionStatsExport, HISTOGRAM_NUM_BUCKETS, LatencyHistogram, MetricsSink,
    OBSERVABILITY_SCHEMA_VERSION, PROFILER_COMMAND_COOKBOOK, QueryPhaseRecord, ReadinessStatus,
    TelemetryRegistry, TraceBenchmarkProvenance, compute_plan_hash, export_otel_span_attributes,
    next_query_execution_id, redact_query_text, validate_metric_label,
};
use lin::{Db, Embedder, HashingEmbedder};

// ===========================================================================
// D40: Structured Error Codes, Context, Cause Chains, Redaction & Warnings
// ===========================================================================

#[test]
fn d40_all_error_codes_roundtrip_and_unique() {
    let expected = [
        (ErrorCode::ParseSyntax, "E_PARSE_SYNTAX"),
        (ErrorCode::ParseUnterminatedStr, "E_PARSE_UNTERMINATED_STR"),
        (ErrorCode::ParseTrailing, "E_PARSE_TRAILING"),
        (ErrorCode::CheckSchema, "E_CHECK_SCHEMA"),
        (ErrorCode::CheckType, "E_CHECK_TYPE"),
        (ErrorCode::CorruptData, "E_CORRUPT_DATA"),
        (ErrorCode::UnsupportedFormat, "E_UNSUPPORTED_FORMAT"),
        (ErrorCode::NotFound, "E_NOT_FOUND"),
        (ErrorCode::MissingSchema, "E_MISSING_SCHEMA"),
        (ErrorCode::Canceled, "E_CANCELED"),
        (ErrorCode::DeadlineExceeded, "E_DEADLINE_EXCEEDED"),
        (ErrorCode::ResourceLimit, "E_RESOURCE_LIMIT"),
        (ErrorCode::AllocationFailure, "E_ALLOCATION_FAILURE"),
        (ErrorCode::UniqueViolation, "E_UNIQUE_VIOLATION"),
        (ErrorCode::FkViolation, "E_FK_VIOLATION"),
        (ErrorCode::CasMismatch, "E_CAS_MISMATCH"),
        (ErrorCode::IncompatibleModel, "E_INCOMPATIBLE_MODEL"),
        (ErrorCode::StalePrepared, "E_STALE_PREPARED"),
        (ErrorCode::MissingFeature, "E_MISSING_FEATURE"),
        (ErrorCode::PermissionDenied, "E_PERMISSION_DENIED"),
        (ErrorCode::Io, "E_IO"),
        (ErrorCode::Gpu, "E_GPU"),
        (ErrorCode::Runtime, "E_RUNTIME"),
    ];

    assert_eq!(ErrorCode::ALL.len(), expected.len());
    let mut seen = HashSet::new();
    for (code, text) in expected {
        assert_eq!(code.as_str(), text);
        assert_eq!(code.to_string(), text);
        assert_eq!(ErrorCode::from_code_str(text), Some(code));
        assert!(seen.insert(text), "duplicate code string {text}");

        let json = serde_json::to_string(&code).expect("serialize ErrorCode");
        let decoded: ErrorCode = serde_json::from_str(&json).expect("deserialize ErrorCode");
        assert_eq!(decoded, code);
    }
    assert_eq!(ErrorCode::from_code_str("E_DOES_NOT_EXIST"), None);
}

#[test]
fn d40_stack_footprint_and_cold_unboxed_fast_path() {
    assert!(
        std::mem::size_of::<Error>() <= 56,
        "Error stack size {} must remain compact",
        std::mem::size_of::<Error>()
    );

    let plain_parse = Error::parse("expected identifier");
    let plain_check = Error::check("unknown field: foo");
    let plain_runtime = Error::runtime("duplicate id: u1");
    assert!(!plain_parse.has_boxed_details());
    assert!(!plain_check.has_boxed_details());
    assert!(!plain_runtime.has_boxed_details());

    assert_eq!(plain_parse.code(), ErrorCode::ParseSyntax);
    assert_eq!(plain_parse.kind(), ErrorKind::Parse);
    assert_eq!(plain_check.code(), ErrorCode::CheckSchema);
    assert_eq!(plain_check.kind(), ErrorKind::Check);
    assert_eq!(plain_runtime.code(), ErrorCode::UniqueViolation);
    assert_eq!(plain_runtime.kind(), ErrorKind::Runtime);
}

#[test]
fn d40_fine_grained_taxonomy_distinctions_and_remediation() {
    // Corrupt vs UnsupportedFormat (`LIN-1565`)
    let corrupt = Error::corrupt("WAL frame CRC32 mismatch");
    let unsupported = Error::unsupported_format("snapshot header version 99 unsupported");
    assert_eq!(corrupt.code(), ErrorCode::CorruptData);
    assert!(corrupt.is_corrupt());
    assert!(!corrupt.is_retryable());
    assert_eq!(unsupported.code(), ErrorCode::UnsupportedFormat);
    assert!(!unsupported.is_corrupt());
    assert!(!unsupported.is_retryable());

    // NotFound vs MissingSchema (`LIN-1566`)
    let not_found = Error::not_found("row id `doc-404` missing");
    let missing_schema = Error::missing_schema("collection `invoices` is not declared");
    assert_eq!(not_found.code(), ErrorCode::NotFound);
    assert_eq!(not_found.phase, Phase::Runtime);
    assert_eq!(missing_schema.code(), ErrorCode::MissingSchema);
    assert_eq!(missing_schema.phase, Phase::Compile);

    // Canceled vs DeadlineExceeded (`LIN-1567`)
    let canceled = Error::canceled("caller cancellation token triggered");
    let deadline = Error::deadline_exceeded("query exceeded 250ms budget");
    assert_eq!(canceled.code(), ErrorCode::Canceled);
    assert!(canceled.is_timeout_or_canceled());
    assert!(!canceled.is_retryable());
    assert_eq!(deadline.code(), ErrorCode::DeadlineExceeded);
    assert!(deadline.is_timeout_or_canceled());
    assert!(deadline.is_retryable());

    // ResourceLimit vs AllocationFailure (`LIN-1568`)
    let limit = Error::resource_limit("max_rows quota 10000 exceeded");
    let alloc = Error::allocation_failure("arena mmap failed for 128 MiB chunk");
    assert_eq!(limit.code(), ErrorCode::ResourceLimit);
    assert_eq!(alloc.code(), ErrorCode::AllocationFailure);
    assert_ne!(limit.code(), alloc.code());

    // Actionable remediation hints (`LIN-1577`..`LIN-1580`)
    let feat = Error::missing_feature("gpu");
    assert_eq!(feat.code(), ErrorCode::MissingFeature);
    assert!(feat.suggestion().unwrap().contains("--features gpu"));

    let stale = Error::stale_prepared("catalog gen 7 != prepared gen 5");
    assert_eq!(stale.code(), ErrorCode::StalePrepared);
    assert!(stale.is_retryable());
    assert!(stale.suggestion().unwrap().contains("prepare"));

    let model = Error::incompatible_model("expected nomic-embed-text/768, got bge-small/384");
    assert_eq!(model.code(), ErrorCode::IncompatibleModel);
    assert!(model.suggestion().unwrap().contains("reembed"));

    // GPU vs CPU classification (`LIN-1599`, `LIN-1600`)
    let gpu = Error::gpu("wgpu compute device lost");
    assert!(gpu.is_gpu_error());
    assert!(gpu.is_retryable());
    assert!(!limit.is_gpu_error());
}

#[test]
fn d40_context_attachments_and_nested_cause_chain_preservation() {
    let root_io = io::Error::new(io::ErrorKind::UnexpectedEof, "short read at disk block 19");
    let mid_err = Error::io("segment page read failed")
        .with_operation("read_segment_page")
        .with_source(root_io);
    let top_err = Error::corrupt("malformed vector payload in WAL replay")
        .with_span(12, 28)
        .with_operation("replay_wal_batch")
        .with_collection("docs")
        .with_field("embedding")
        .with_constraint("chk_docs_embedding_dim")
        .with_wal_offset(1_048_576)
        .with_vec_offset(383)
        .with_suggestion("truncate trailing partial WAL frame or restore snapshot")
        .with_source(mid_err);

    assert!(top_err.has_boxed_details());
    assert_eq!(top_err.code(), ErrorCode::CorruptData);
    assert_eq!(top_err.span(), Some((12, 28)));
    assert_eq!(top_err.pos, Some(12));
    assert_eq!(top_err.operation(), Some("replay_wal_batch"));
    assert_eq!(top_err.collection(), Some("docs"));
    assert_eq!(top_err.field(), Some("embedding"));
    assert_eq!(top_err.constraint(), Some("chk_docs_embedding_dim"));
    assert_eq!(top_err.wal_offset(), Some(1_048_576));
    assert_eq!(top_err.vec_offset(), Some(383));
    assert_eq!(
        top_err.suggestion(),
        Some("truncate trailing partial WAL frame or restore snapshot")
    );

    // Traverse via standard `std::error::Error::source` (`LIN-1562`, `LIN-1569`)
    let s1 = top_err.source().expect("first cause");
    assert!(s1.to_string().contains("segment page read failed"));
    let s2 = s1.source().expect("second cause");
    assert!(s2.to_string().contains("short read at disk block 19"));
    assert!(s2.source().is_none());

    let chain = top_err.cause_chain();
    assert_eq!(chain.len(), 2);

    let report = top_err.diagnostic_report();
    assert!(report.contains("[E_CORRUPT_DATA]"));
    assert!(report.contains("[collection=docs]"));
    assert!(report.contains("[field=embedding]"));
    assert!(report.contains("[constraint=chk_docs_embedding_dim]"));
    assert!(report.contains("[wal_offset=1048576]"));
    assert!(report.contains("[vec_offset=383]"));
    assert!(report.contains("caused by:"));
}

#[test]
fn d40_golden_diagnostic_corpora_parser_storage_replication_and_parity() {
    // Parser & checker golden corpus against real compiler (`LIN-1589`, `LIN-1590`)
    let e_unterm = lin::parse(r#"docs | title == "unterminated"#).unwrap_err();
    assert_eq!(e_unterm.code(), ErrorCode::ParseUnterminatedStr);

    let e_trailing = lin::parse("docs | { id } extra_garbage_tokens").unwrap_err();
    assert_eq!(e_trailing.code(), ErrorCode::ParseTrailing);

    let e_syntax = lin::parse("docs | == 123").unwrap_err();
    assert_eq!(e_syntax.code(), ErrorCode::ParseSyntax);

    let e_missing_col = lin::compile("nonexistent_collection_xyz | { id }").unwrap_err();
    assert_eq!(e_missing_col.code(), ErrorCode::MissingSchema);

    let e_check_field = lin::compile("docs | { unknown_field_xyz }").unwrap_err();
    assert_eq!(e_check_field.code(), ErrorCode::CheckSchema);

    // Storage & replication golden corpus (`LIN-1591`, `LIN-1592`)
    let storage_cases = [
        (Error::runtime("wal: crc mismatch at frame 4"), ErrorCode::CorruptData, false),
        (Error::runtime("snapshot: unsupported format version 12"), ErrorCode::UnsupportedFormat, false),
        (Error::runtime("duplicate id: doc-1"), ErrorCode::UniqueViolation, false),
        (Error::runtime("foreign key violation on orders.user_id"), ErrorCode::FkViolation, false),
        (Error::runtime("cas mismatch: expected v3, found v4"), ErrorCode::CasMismatch, true),
        (Error::runtime("ship pull io error: connection reset by peer"), ErrorCode::Io, true),
        (Error::runtime("read-only follower: writes not allowed"), ErrorCode::PermissionDenied, false),
    ];
    for (err, expected_code, expected_retry) in storage_cases {
        assert_eq!(err.code(), expected_code, "mismatch for `{err}`");
        assert_eq!(err.is_retryable(), expected_retry, "retryable mismatch for `{err}`");
    }

    // Library / CLI report parity (`LIN-1597`)
    let mut db = Db::fixture();
    let sync_err = db.run("missing_table | { id }").unwrap_err();
    assert_eq!(sync_err.code(), ErrorCode::MissingSchema);
    assert!(sync_err.diagnostic_report().starts_with("[E_MISSING_SCHEMA]"));
}

#[cfg(feature = "async")]
#[test]
fn d40_async_and_sync_error_parity() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(async {
        let mut sync_db = Db::fixture();
        let async_db = lin::AsyncDb::new(Db::fixture());

        let sync_err = sync_db.run("missing_table | { id }").unwrap_err();
        let async_err = async_db.run("missing_table | { id }").await.unwrap_err();

        assert_eq!(sync_err.code(), async_err.code());
        assert_eq!(sync_err.phase, async_err.phase);
        assert_eq!(sync_err.to_string(), async_err.to_string());
    });
}

#[test]
fn d40_credential_and_sensitive_path_redaction() {
    let raw = "connect postgres://admin:Sup3rS3cret!@db.internal:5432/prod?token=tok_live_998877&mode=tls \
               Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload.sig \
               s3://AKIA123:awsSecretKey99@bucket/seg password='my_password_123'";
    let redacted = redact_credentials(raw);

    assert!(!redacted.contains("Sup3rS3cret!"));
    assert!(!redacted.contains("tok_live_998877"));
    assert!(!redacted.contains("eyJhbGciOiJIUzI1NiJ9"));
    assert!(!redacted.contains("awsSecretKey99"));
    assert!(!redacted.contains("my_password_123"));
    assert!(redacted.contains("postgres://admin:[REDACTED]@db.internal:5432/prod"));
    assert!(redacted.contains("token=[REDACTED]"));
    assert!(redacted.contains("Bearer [REDACTED]"));
    assert!(redacted.contains("password='[REDACTED]'"));

    // Path redaction (`LIN-1570`)
    let home_wal = Path::new("/Users/alice_secret_user/projects/lin_store/000042.wal");
    let red_wal = redact_sensitive_path(home_wal);
    assert!(!red_wal.contains("alice_secret_user"));
    assert!(!red_wal.contains("/Users"));
    assert_eq!(red_wal, ".../lin_store/000042.wal");

    let direct_home = Path::new("/home/bob_admin/backup.snap");
    let red_home = redact_sensitive_path(direct_home);
    assert!(!red_home.contains("bob_admin"));
    assert_eq!(red_home, ".../[REDACTED]/backup.snap");

    let secret_dir = Path::new("/var/lib/lin/.aws/credentials");
    let red_secret = redact_sensitive_path(secret_dir);
    assert!(!red_secret.contains(".aws"));
    assert_eq!(red_secret, ".../[REDACTED]/credentials");
}

#[test]
fn d40_warning_channel_severity_suppression_and_budget() {
    let mut chan = WarningChannel::new(3).with_min_severity(WarningSeverity::Low);
    chan.suppress(WarningSuppressionScope::Code("W_DEPRECATED_ALIAS".into()));
    chan.suppress(WarningSuppressionScope::Collection("scratch_tmp".into()));

    // Filtered by min_severity (`Info < Low`)
    assert!(!chan.emit(DiagnosticWarning::new(
        "W_HINT",
        WarningSeverity::Info,
        "implicit take 50 applied"
    )));
    // Filtered by suppressed code
    assert!(!chan.emit(DiagnosticWarning::new(
        "W_DEPRECATED_ALIAS",
        WarningSeverity::High,
        "legacy alias"
    )));
    // Filtered by suppressed collection
    assert!(!chan.emit(
        DiagnosticWarning::new("W_UNINDEXED", WarningSeverity::Medium, "full scan")
            .with_collection("scratch_tmp")
    ));
    assert_eq!(chan.suppressed_by_policy_count(), 3);
    assert_eq!(chan.warnings().len(), 0);

    // Emit 5 valid warnings into a budget of 3 (`LIN-1588`)
    for idx in 0..5 {
        let accepted = chan.emit(
            DiagnosticWarning::new(
                "W_FULL_SCAN",
                WarningSeverity::Medium,
                format!("scan #{idx}"),
            )
            .with_collection("orders")
            .with_operation("query"),
        );
        assert_eq!(accepted, idx < 3);
    }

    assert_eq!(chan.warnings().len(), 3);
    assert_eq!(chan.dropped_over_budget_count(), 2);
    assert_eq!(chan.suppressed_count(), 5);

    let drained = chan.drain();
    assert_eq!(drained.len(), 3);
    assert!(chan.warnings().is_empty());
}

// ===========================================================================
// D41 & D35: Runtime Observability, Histograms, Concurrency & Prometheus/OTel
// ===========================================================================

#[test]
fn d41_histogram_quantiles_bounds_and_reset() {
    let hist = LatencyHistogram::new();
    assert_eq!(hist.count(), 0);
    assert_eq!(hist.p50_ns(), 0);
    assert_eq!(hist.min_ns(), 0);

    // Record across sub-microsecond, millisecond, and >8s overflow buckets
    for _ in 0..50 {
        hist.record_ns(500); // < 1 µs bucket 0
    }
    for _ in 0..45 {
        hist.record_ns(2_000_000); // 2 ms
    }
    for _ in 0..5 {
        hist.record_ns(10_000_000_000); // 10 s (> 8 s overflow bucket 23)
    }

    assert_eq!(hist.count(), 100);
    assert_eq!(hist.min_ns(), 500);
    assert_eq!(hist.max_ns(), 10_000_000_000);
    assert!(hist.min_ns() <= hist.p50_ns());
    assert!(hist.p50_ns() <= hist.p95_ns());
    assert!(hist.p95_ns() <= hist.p99_ns());
    assert!(hist.p99_ns() <= hist.max_ns());
    assert_eq!(hist.bucket_counts()[HISTOGRAM_NUM_BUCKETS - 1], 5);

    hist.reset();
    assert_eq!(hist.count(), 0);
    assert_eq!(hist.sum_ns(), 0);
    assert_eq!(hist.min_ns(), 0);
    assert_eq!(hist.max_ns(), 0);
}

#[test]
fn d41_query_redaction_label_cardinality_and_phase_anomalies() {
    let raw_query = r#"users_v2 | email == "alice@corp.internal" and secret == 'p\"ass' and balance >= -4250.75 and ts > ago 7d and token == 0xdeadbeef | { id, $tenant }"#;
    let redacted = redact_query_text(raw_query);

    assert!(!redacted.contains("alice@corp.internal"));
    assert!(!redacted.contains("4250"));
    assert!(!redacted.contains("deadbeef"));
    assert!(!redacted.contains("7d"));
    assert!(redacted.contains("users_v2"));
    assert!(redacted.contains("$tenant"));
    assert!(redacted.contains("email == ?"));
    assert!(redacted.contains("balance >= ?"));

    // Metric label validation (`LIN-1618`, `LIN-1620`)
    assert!(validate_metric_label("op", "scan").is_ok());
    assert!(validate_metric_label("role", "primary").is_ok());
    assert!(validate_metric_label("user_id", "alice").is_err());
    assert!(validate_metric_label("query_id", "q1").is_err());
    assert!(validate_metric_label("__internal", "ok").is_err());
    assert!(validate_metric_label("collection", "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3").is_err());
    assert!(validate_metric_label("collection", "0123456789abcdef0123").is_err());
    assert!(validate_metric_label("endpoint", "https://evil.example/x").is_err());
    assert!(validate_metric_label("owner", "alice@example.com").is_err());

    // Phase anomaly detection (`LIN-1623`)
    let queue_skew = QueryPhaseRecord::new(1, 42, 1_000, 100_000, 0, 20_000_000, 10, 10, 512, "docs");
    assert!(queue_skew.anomaly_hint.unwrap().contains("queue_contention"));

    let commit_skew = QueryPhaseRecord::new(2, 42, 1_000, 100_000, 25_000_000, 0, 1, 1, 512, "insert docs");
    assert!(commit_skew.anomaly_hint.unwrap().contains("commit_stall"));

    let unselective = QueryPhaseRecord::new(3, 42, 1_000, 500_000, 0, 0, 50_000, 2, 1024, "docs | x == 1");
    assert!(unselective.anomaly_hint.unwrap().contains("unselective_scan"));
}

struct PanickingSink {
    calls: AtomicU64,
}

impl MetricsSink for PanickingSink {
    fn on_query(&self, _record: &QueryPhaseRecord) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        panic!("simulated metrics sink explosion in on_query");
    }

    fn on_counter(&self, _name: &str, _delta: u64) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        panic!("simulated metrics sink explosion in on_counter");
    }

    fn on_gauge(&self, _name: &str, _value: i64) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        panic!("simulated metrics sink explosion in on_gauge");
    }
}

#[test]
fn d41_eight_thread_concurrency_panic_isolation_and_shutdown_safety() {
    let registry = Arc::new(TelemetryRegistry::with_slow_ring(32, 50_000_000, 10));
    let sink = Arc::new(PanickingSink {
        calls: AtomicU64::new(0),
    });
    registry.set_sink(Some(sink.clone()));

    // Verify RAII ActiveQueryGuard during caller panic unwind + panicking sink (`LIN-1633`, `LIN-1635`)
    let unwind_res = catch_unwind(AssertUnwindSafe(|| {
        let _guard = registry.enter_query();
        assert_eq!(registry.active_queries.load(Ordering::Relaxed), 1);
        panic!("simulated operator panic");
    }));
    assert!(unwind_res.is_err());
    assert_eq!(registry.active_queries.load(Ordering::Relaxed), 0);
    assert!(registry.sink_failure_count() >= 2);

    // 8 threads hammering registry concurrently (`LIN-1634`)
    let threads = 8usize;
    let iters = 250u64;
    let mut handles = Vec::with_capacity(threads);
    for t in 0..threads {
        let reg = Arc::clone(&registry);
        handles.push(thread::spawn(move || {
            let plan_hash = compute_plan_hash("Scan docs -> Take 50");
            for i in 0..iters {
                let _g = reg.enter_query();
                let exec_id = next_query_execution_id();
                let exec_ns = if i % 50 == 0 { 60_000_000 } else { 100_000 };
                let rec = QueryPhaseRecord::new(
                    exec_id,
                    plan_hash,
                    10_000,
                    exec_ns,
                    5_000,
                    2_000,
                    100,
                    10,
                    4096,
                    &format!("docs | tenant == \"t-{t}\" and seq == {i}"),
                );
                reg.record_query(rec);
                reg.record_replica_reconnect();
                reg.record_stale_snapshot();
                reg.record_embedding_failure();
                reg.record_compaction_pause(1_000);
            }
        }));
    }
    for h in handles {
        h.join().expect("worker thread must never panic despite panicking sink");
    }

    let total_ops = (threads as u64) * iters;
    assert_eq!(registry.prepare_latency.count(), total_ops);
    assert_eq!(registry.execute_latency.count(), total_ops);
    assert_eq!(registry.operator_rows_in.load(Ordering::Relaxed), total_ops * 100);
    assert_eq!(registry.operator_rows_out.load(Ordering::Relaxed), total_ops * 10);
    assert_eq!(
        registry.operator_bytes_allocated.load(Ordering::Relaxed),
        total_ops * 4096
    );
    assert_eq!(registry.replica_reconnects.load(Ordering::Relaxed), total_ops);
    assert_eq!(registry.stale_snapshots.load(Ordering::Relaxed), total_ops);
    assert_eq!(registry.embedding_failures.load(Ordering::Relaxed), total_ops);
    assert_eq!(registry.compaction_pauses.load(Ordering::Relaxed), total_ops);
    assert_eq!(registry.active_queries.load(Ordering::Relaxed), 0);
    assert!(sink.calls.load(Ordering::Relaxed) > 0);

    // Bounded slow-query ring never exceeds capacity 32 and contains zero raw literals (`LIN-1619`, `LIN-1621`, `LIN-1622`)
    let slow_snap = registry.slow_queries.snapshot();
    assert_eq!(slow_snap.len(), 32);
    assert!(registry.slow_queries.evicted_count() > 0);
    for entry in &slow_snap {
        assert!(!entry.redacted_query.contains("t-"));
        assert!(entry.redacted_query.contains("tenant == ?"));
    }

    // Resettable operational counters (`LIN-1632`)
    registry.reset_counters();
    assert_eq!(registry.operator_rows_in.load(Ordering::Relaxed), 0);
    assert_eq!(registry.replica_reconnects.load(Ordering::Relaxed), 0);
    assert_eq!(registry.sink_failure_count(), 0);

    // Post-shutdown instrumentation safety (`LIN-1636`)
    registry.mark_shutdown();
    assert!(registry.is_shutting_down());
    assert_eq!(registry.readiness_status(), ReadinessStatus::Draining);
    let sink_calls_before = sink.calls.load(Ordering::Relaxed);
    registry.record_replica_reconnect();
    registry.set_wal_pending_bytes(4096);
    assert_eq!(
        sink.calls.load(Ordering::Relaxed),
        sink_calls_before,
        "detached sink must not be invoked after mark_shutdown()"
    );
    assert!(!registry.export_prometheus_text().is_empty());

    // Disabled fast path (`LIN-1617`)
    let disabled = TelemetryRegistry::disabled();
    assert!(!disabled.is_enabled());
    disabled.record_replica_reconnect();
    assert_eq!(disabled.replica_reconnects.load(Ordering::Relaxed), 0);
}

#[test]
fn d41_health_readiness_prometheus_exposition_and_otel_attributes() {
    assert_eq!(OBSERVABILITY_SCHEMA_VERSION, 1);
    assert!(!PROFILER_COMMAND_COOKBOOK.is_empty());

    let reg = TelemetryRegistry::with_slow_ring(16, 1_000_000, 1);
    reg.set_readiness_thresholds(1_000_000, 10_000_000);
    reg.set_wal_pending_bytes(512_000);
    reg.set_checkpoint_debt_bytes(128_000);
    reg.set_pool_retained_bytes(2_097_152);
    reg.set_gpu_resident_bytes(4_194_304);
    reg.set_replica_lag(15, 4_096);

    assert_eq!(reg.readiness_status(), ReadinessStatus::Ready);
    let health = reg.health_snapshot();
    assert!(health.healthy);
    assert!(!health.shutting_down);
    assert_eq!(health.schema_version, OBSERVABILITY_SCHEMA_VERSION);

    // Replica CatchingUp (`LIN-1385`, `LIN-1386`, `LIN-1630`)
    reg.set_replica_lag(450, 2_500_000);
    assert_eq!(
        reg.readiness_status(),
        ReadinessStatus::CatchingUp {
            lag_bytes: 2_500_000
        }
    );

    // Degraded WAL backlog
    reg.set_wal_pending_bytes(12_000_000);
    assert!(matches!(
        reg.readiness_status(),
        ReadinessStatus::Degraded { .. }
    ));
    assert!(!reg.health_snapshot().healthy);

    // Per-collection export (`LIN-1631`) & provenance (`LIN-1640`)
    reg.update_collection_stats(CollectionStatsExport {
        collection: "docs".into(),
        live_rows: 1200,
        retained_bytes: 98_304,
        scans: 44,
        mutations: 12,
    });
    assert_eq!(reg.export_collection_stats().len(), 1);

    let rec = QueryPhaseRecord::new(
        99,
        compute_plan_hash("Scan docs"),
        150_000,
        2_500_000,
        400_000,
        50_000,
        1200,
        25,
        16_384,
        r#"docs | wing == "rag""#,
    );
    reg.record_query(rec.clone());

    let prov = TraceBenchmarkProvenance {
        exec_id: rec.exec_id,
        plan_hash: rec.plan_hash,
        benchmark_suite: "compare".into(),
        git_commit: "HEAD".into(),
    };
    assert_eq!(prov.exec_id, 99);

    // OTel attributes (`LIN-1615`)
    let otel = export_otel_span_attributes(&rec);
    assert!(otel.iter().any(|(k, v)| *k == "db.system" && v == "lin"));
    assert!(
        otel.iter()
            .any(|(k, v)| *k == "db.statement" && v == "docs | wing == ?")
    );

    // Parse & validate Prometheus 0.0.4 text exposition format (`LIN-1616`)
    let prom = reg.export_prometheus_text();
    let mut help_count = 0usize;
    let mut type_count = 0usize;
    let mut prev_exec_bucket: Option<u64> = None;
    let mut exec_inf_bucket: Option<u64> = None;
    let mut exec_count: Option<u64> = None;

    for line in prom.lines() {
        assert!(!line.is_empty(), "Prometheus exposition should not emit blank lines");
        if line.starts_with("# HELP ") {
            help_count += 1;
        } else if line.starts_with("# TYPE ") {
            type_count += 1;
        } else {
            // Must contain `<metric_and_optional_labels> <numeric_value>`
            let (lhs, rhs) = line
                .rsplit_once(' ')
                .unwrap_or_else(|| panic!("invalid Prometheus sample line: {line}"));
            assert!(!lhs.is_empty());
            rhs.parse::<f64>()
                .unwrap_or_else(|e| panic!("invalid numeric value in `{line}`: {e}"));

            if let Some(rest) = lhs.strip_prefix("lin_execute_latency_seconds_bucket{le=\"") {
                let le_str = rest.strip_suffix("\"}").expect("closing label brace");
                let bucket_val: u64 = rhs.parse().expect("integer bucket count");
                if let Some(prev) = prev_exec_bucket {
                    assert!(
                        bucket_val >= prev,
                        "histogram buckets must be monotonically non-decreasing"
                    );
                }
                prev_exec_bucket = Some(bucket_val);
                if le_str == "+Inf" {
                    exec_inf_bucket = Some(bucket_val);
                }
            } else if lhs == "lin_execute_latency_seconds_count" {
                exec_count = Some(rhs.parse().expect("integer count"));
            }
        }
    }
    assert_eq!(help_count, type_count);
    assert!(help_count >= 20, "expected >= 20 metric families, got {help_count}");
    assert_eq!(exec_inf_bucket, Some(1));
    assert_eq!(exec_count, Some(1));
}

// ===========================================================================
// D45: Security Capabilities, Sandboxed File-Root, Offline & UTF-8 Safety
// ===========================================================================

#[test]
fn d45_capability_policy_acl_unsafe_denial_and_offline_execution() {
    let default_policy = CapabilityPolicy::default();
    assert!(!default_policy.read_only);
    assert!(default_policy.deny_unsafe_ops);
    assert!(!default_policy.allow_network_models);

    // Deny-by-default unsafe ops (`LIN-1786`) & extension trust (`LIN-1779`)
    let err_unsafe = default_policy
        .check_stmt_allowed("load_extension", None)
        .unwrap_err();
    assert_eq!(err_unsafe.code(), ErrorCode::PermissionDenied);
    assert!(
        default_policy
            .check_extension_trust(ExtensionTrustBoundary::Untrusted)
            .is_err()
    );
    assert!(
        default_policy
            .check_extension_trust(ExtensionTrustBoundary::BuiltinVerified)
            .is_ok()
    );

    // Unrecognized statement kind fails closed
    assert_eq!(
        default_policy
            .check_stmt_allowed("arbitrary_backdoor_cmd", Some("docs"))
            .unwrap_err()
            .code(),
        ErrorCode::PermissionDenied
    );

    // Strict read-only mode (`LIN-1777`)
    let ro = CapabilityPolicy::strict_read_only();
    assert!(ro.check_stmt_allowed("query", Some("docs")).is_ok());
    assert!(ro.check_stmt_allowed("explain", None).is_ok());
    assert!(ro.check_stmt_allowed("insert", Some("docs")).is_err());
    assert!(ro.check_stmt_allowed("update", Some("docs")).is_err());
    assert!(ro.check_stmt_allowed("delete", Some("docs")).is_err());
    assert!(ro.check_stmt_allowed("ddl", Some("docs")).is_err());

    // Collection-scoped capabilities & bitmask roundtrip (`LIN-1778`)
    let cap_rw = CollectionCapability::read_write();
    assert_eq!(
        CollectionCapability::from_bitmask(cap_rw.to_bitmask()),
        cap_rw
    );
    assert_eq!(
        CollectionCapability::read_only()
            .union(CollectionCapability::all())
            .intersection(CollectionCapability::read_write()),
        cap_rw
    );

    let scoped = CapabilityPolicy::new()
        .with_collection_capability("public_docs", CollectionCapability::read_only())
        .with_collection_capability("events", CollectionCapability::read_write());

    assert!(scoped.check_stmt_allowed("read", Some("public_docs")).is_ok());
    assert!(scoped.check_stmt_allowed("insert", Some("public_docs")).is_err());
    assert!(scoped.check_stmt_allowed("insert", Some("events")).is_ok());
    assert!(scoped.check_stmt_allowed("ddl", Some("events")).is_err());
    assert!(scoped.check_stmt_allowed("read", Some("secret_payroll")).is_err());

    // Offline execution & explicit network model permissions (`LIN-1780`, `LIN-1787`, `LIN-1796`)
    assert!(default_policy.check_network_model_allowed("hashing").is_ok());
    assert!(default_policy.check_network_model_allowed("local").is_ok());
    for remote in ["ollama", "openai", "huggingface", "hf-hub", "https://models.example"] {
        let e = default_policy
            .check_network_model_allowed(remote)
            .unwrap_err();
        assert_eq!(e.code(), ErrorCode::PermissionDenied);
    }

    let online = CapabilityPolicy::new().with_allow_network_models(true);
    assert!(online.check_network_model_allowed("ollama").is_ok());

    // Verify full local offline Db + HashingEmbedder execution without network
    let mut offline_db = Db::fixture();
    let embedder = HashingEmbedder::new("offline-hash", 64);
    assert_eq!(embedder.embed("airgapped vector query").len(), 64);
    let res = offline_db.run(r#"docs | search "rag" | take 3"#).expect("offline search");
    assert!(!res.rows.is_empty());
}

#[test]
fn d45_sandboxed_import_path_regression_corpus() {
    let uniq = format!(
        "lin_sandbox_qual_{}_{}",
        std::process::id(),
        next_query_execution_id()
    );
    let base_dir: PathBuf = std::env::temp_dir().join(uniq);
    let import_root = base_dir.join("allowed_imports");
    let outside_dir = base_dir.join("outside_secrets");
    std::fs::create_dir_all(import_root.join("nested")).expect("create import_root");
    std::fs::create_dir_all(&outside_dir).expect("create outside_dir");

    let secret_file = outside_dir.join("passwd.csv");
    std::fs::write(&secret_file, "root:x:0:0\n").expect("write outside file");
    let valid_file = import_root.join("nested/batch_01.parquet");
    std::fs::write(&valid_file, b"PAR1").expect("write valid file");

    let policy = CapabilityPolicy::new().with_allowed_import_root(&import_root);

    // Valid relative & canonical in-root paths succeed
    let resolved_rel = policy
        .validate_sandboxed_path(Path::new("nested/batch_01.parquet"))
        .expect("valid relative import path");
    assert!(resolved_rel.ends_with("nested/batch_01.parquet"));

    let resolved_new_out = policy
        .validate_sandboxed_path(Path::new("nested/future_export.jsonl"))
        .expect("valid non-existent child inside sandbox");
    assert!(resolved_new_out.ends_with("nested/future_export.jsonl"));

    // Path traversal, NUL byte, backslash, and root-escape regression corpus (`LIN-1788`, `LIN-1793`)
    let rejected_paths: &[&Path] = &[
        Path::new(""),
        Path::new("../outside_secrets/passwd.csv"),
        Path::new("nested/../../outside_secrets/passwd.csv"),
        Path::new("..\\..\\etc\\passwd"),
        Path::new("/etc/passwd"),
        Path::new("nested/evil\0.csv"),
        &secret_file,
    ];
    for &bad in rejected_paths {
        let err = policy.validate_sandboxed_path(bad).unwrap_err();
        assert_eq!(
            err.code(),
            ErrorCode::PermissionDenied,
            "expected PermissionDenied for {:?}",
            bad
        );
    }

    // Unix symlink escape (live target outside sandbox AND dangling symlink outside sandbox)
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let escape_link = import_root.join("nested/escape_link.csv");
        symlink(&secret_file, &escape_link).expect("create symlink");
        let err = policy.validate_sandboxed_path(&escape_link).unwrap_err();
        assert_eq!(err.code(), ErrorCode::PermissionDenied);

        let dangling_escape = import_root.join("nested/dangling_escape.csv");
        symlink(outside_dir.join("nonexistent_secret.key"), &dangling_escape)
            .expect("create dangling symlink");
        let err_dangling = policy
            .validate_sandboxed_path(&dangling_escape)
            .unwrap_err();
        assert_eq!(err_dangling.code(), ErrorCode::PermissionDenied);
    }

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[test]
fn d45_resource_limit_receipts_utf8_scanner_and_ffi_provenance() {
    // ResourceLimitReceipt (`LIN-1785`)
    let receipt = ResourceLimitReceipt::new(
        "max_query_memory_bytes",
        67_108_864,
        83_886_080,
        Some("events".into()),
        "increase `Quotas::max_memory_bytes` or add `take` limit",
    );
    assert_eq!(receipt.exceeded_by(), 16_777_216);
    let err = receipt.to_error();
    assert_eq!(err.code(), ErrorCode::ResourceLimit);
    assert_eq!(err.collection(), Some("events"));
    assert!(err.suggestion().unwrap().contains("Quotas"));

    // Pathological UTF-8 / control-char O(N) scanner (`LIN-1783`)
    assert_eq!(
        scan_utf8_safety("oversized payload", 8).unwrap_err().code(),
        ErrorCode::ResourceLimit
    );
    let nul_err = scan_utf8_safety("abc\0def", 64).unwrap_err();
    assert_eq!(nul_err.code(), ErrorCode::CorruptData);
    assert_eq!(nul_err.span(), Some((3, 4)));

    // Trojan-Source bidi & control char detection
    let bidi = "safe\u{202E}evil\u{2066}\x07\u{200B}\u{FFFD}";
    let sum_bidi = scan_utf8_safety(bidi, 1024).expect("bidi scan");
    assert!(sum_bidi.has_suspicious_patterns());
    assert_eq!(sum_bidi.bidi_override_count, 2);
    assert_eq!(sum_bidi.control_char_count, 1);
    assert_eq!(sum_bidi.zero_width_count, 1);
    assert_eq!(sum_bidi.replacement_char_count, 1);

    // 200,000-byte pathological Zalgo combining mark storm + 4-byte emoji storm in single O(N) pass
    let mut pathological = String::with_capacity(260_000);
    pathological.push('a');
    for _ in 0..50_000 {
        pathological.push('\u{0300}'); // 2-byte combining grave accent
    }
    for _ in 0..25_000 {
        pathological.push('🦀'); // 4-byte UTF-8 scalar
    }
    let t0 = Instant::now();
    let summary = scan_utf8_safety(&pathological, 512 * 1024).expect("pathological scan");
    let elapsed = t0.elapsed();
    assert!(
        elapsed.as_millis() < 500,
        "O(N) UTF-8 scan took unexpectedly long: {elapsed:?}"
    );
    assert_eq!(summary.max_combining_run, 50_000);
    assert_eq!(summary.multibyte_chars, 75_000);
    assert!(summary.has_suspicious_patterns());

    // C ABI buffer descriptor validation, provenance inventory & disclosure (`LIN-1789`, `LIN-1794`, `LIN-1795`)
    assert_eq!(validate_c_abi_buffer_descriptor(0x1000, 8, 4, 16).unwrap(), 64);
    assert!(validate_c_abi_buffer_descriptor(0, 8, 4, 16).is_err()); // null
    assert!(validate_c_abi_buffer_descriptor(0x1003, 8, 4, 16).is_err()); // unaligned
    assert!(validate_c_abi_buffer_descriptor(0x1000, 8, usize::MAX, 4).is_err()); // overflow
    assert!(!DEPENDENCY_PROVENANCE_INVENTORY.is_empty());
    assert!(RESPONSIBLE_DISCLOSURE_GUIDANCE.contains("72 hours"));
}
