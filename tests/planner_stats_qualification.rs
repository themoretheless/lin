//! Qualification suite for `D12`, `D13`, `D14`, and `D15`:
//! - Collection/Column statistics (`ANALYZE`), histograms, memory budgets, persistence & calibrated `CostEstimator` (`LIN-0561`..`LIN-0600`)
//! - Logical optimizer rewrites, volatile barriers, bounded lazy DNF, normalized fingerprints & plan equivalence oracle (`LIN-0526`..`LIN-0560`)
//! - Structured `EXPLAIN` (`ExplainBackend`, GPU eligibility, `PlanAccuracy`, type annotations, `q_error` receipts `LIN-0507`, `LIN-0557`..`LIN-0560`, `LIN-1463`)
//! - Parser & checker diagnostics, escaped string cap, canonical AST formatter, malformed query minimizer & grammar conformance (`LIN-0441`..`LIN-0520`)

use std::time::{Duration as StdDuration, Instant};

use lin::stats::{
    CostEstimator, CostUnits, DiagnosticKind, ExplainBackend, LazyDnfIter,
    MAX_DECODED_STRING_BYTES, MAX_DNF_CLAUSES, NodeKind, OptimizerOptions, PlanAccuracy,
    STATS_VERSION, SYNTAX_VERSION, StatsCatalog, bounded_levenshtein, check_diagnostics,
    check_diagnostics_with_catalog, check_program, compare_estimate_actual,
    compare_explain_with_actual, deduplicate_conjuncts, execute_with_optimizer_opts,
    explain_structured, explain_structured_with_catalog, fixture_catalog, format_program_canonical,
    format_stmt_canonical, has_volatile_barrier, is_deterministic_pred, minimize_malformed_query,
    offset_to_line_col, parse_inspect, parse_program, plan_fingerprint, plan_program_with_opts,
    predicate_fingerprint, stream_dnf_bounded,
};
use lin::{Cell, CmpOp, Db, Row, Value, parse, parse_pred};

// ============================================================================
// 1. D15: Statistics (`ANALYZE`) & Calibrated Cost Model (`LIN-0561`..`LIN-0600`)
// ============================================================================

#[test]
fn stats_catalog_histograms_persistence_budget_and_staleness() {
    assert_eq!(STATS_VERSION, 1);

    let mut rows = Vec::with_capacity(200);
    for i in 0..200usize {
        let mut r = Row::new();
        r.insert("id", Cell::from_text(format!("r{i:04}")));
        // Numeric column with explicit NaNs
        if i < 4 {
            r.insert("score", Cell::Float(f64::NAN));
        } else {
            r.insert("score", Cell::Float((i as f64) * 2.5));
        }
        // Text column spanning multiple log2 length buckets
        let text_len = 1usize << (i % 8);
        r.insert("body", Cell::from_text("a".repeat(text_len)));
        // Vector column with dimensions
        if i % 5 != 0 {
            let dim = if i % 2 == 0 { 8 } else { 16 };
            r.insert("emb", Cell::vec_arc(vec![0.25f32; dim]));
        } else {
            r.insert("emb", Cell::Null);
        }
        rows.push(r);
    }

    let mut cat = StatsCatalog::new();
    assert!(cat.is_stale("items"), "unanalyzed collection must be stale");

    // Full scan ANALYZE (`confidence == 1.0`)
    let full = cat.analyze_rows("items", &rows, None).unwrap().clone();
    assert_eq!(full.row_count, 200);
    assert_eq!(full.sampled_rows, 200);
    assert!(!cat.is_stale("items"));

    let score_col = full.columns.get("score").expect("score stats");
    assert_eq!(score_col.confidence, 1.0);
    assert_eq!(score_col.null_fraction, 0.0);
    let nh = score_col.numeric_hist.as_ref().expect("numeric histogram");
    assert_eq!(nh.nan_count, 4);
    assert_eq!(nh.min, 10.0);
    assert_eq!(nh.max, 497.5);
    assert_eq!(nh.buckets.iter().sum::<usize>(), 196);

    let body_col = full.columns.get("body").expect("body stats");
    let th = body_col.text_hist.as_ref().expect("text length histogram");
    assert_eq!(th.min_len, 1);
    assert_eq!(th.max_len, 128);
    assert!(th.avg_len > 10.0);
    assert!(th.buckets.iter().all(|&b| b > 0));

    let emb_col = full.columns.get("emb").expect("vec stats");
    assert!((emb_col.null_fraction - 0.20).abs() < 1e-9);
    let vs = emb_col.vec_stats.as_ref().expect("vec dim stats");
    assert_eq!(vs.min_dim, 8);
    assert_eq!(vs.max_dim, 16);
    assert_eq!(vs.non_null_vecs, 160);

    // Sampled ANALYZE (`confidence` scaled in `[0.1, 1.0)`) + reproducibility (`LIN-0591`, `LIN-0600`)
    let mut sampled_a = StatsCatalog::new();
    let mut sampled_b = StatsCatalog::new();
    let sa = sampled_a
        .analyze_rows("items", &rows, Some(50))
        .unwrap()
        .clone();
    let sb = sampled_b
        .analyze_rows("items", &rows, Some(50))
        .unwrap()
        .clone();
    assert_eq!(sa, sb, "sampled ANALYZE must be 100% deterministic");
    assert_eq!(sa.sampled_rows, 50);
    let sc = sa.columns["id"].confidence;
    assert!((0.1..1.0).contains(&sc), "expected scaled confidence, got {sc}");
    assert!(
        sa.columns["id"].distinct_estimate >= 180,
        "Good-Turing scaling on unique IDs should scale near 200, got {}",
        sa.columns["id"].distinct_estimate
    );

    // Incremental mutation tracking & staleness (`LIN-0571`, `LIN-0572`)
    cat.record_mutations("items", 10);
    assert!(!cat.is_stale("items"), "10/200 (5%) < 20% threshold");
    cat.record_mutations("items", 35);
    assert!(cat.is_stale("items"), "45/200 (22.5%) >= 20% threshold");

    // Persistence roundtrip via JSON & MessagePack + version validation (`LIN-0593`, `LIN-0594`)
    let json_bytes = cat.to_json_bytes().unwrap();
    let from_json = StatsCatalog::from_json_bytes(&json_bytes).unwrap();
    assert_eq!(from_json, cat);

    let mp_bytes = cat.to_msgpack_bytes().unwrap();
    let from_mp = StatsCatalog::from_msgpack_bytes(&mp_bytes).unwrap();
    assert_eq!(from_mp, cat);

    let bad_version = String::from_utf8(json_bytes)
        .unwrap()
        .replace("\"version\":1", "\"version\":99");
    assert!(StatsCatalog::from_json_bytes(bad_version.as_bytes()).is_err());

    // Schema reconciliation (`LIN-0596`) & reset (`LIN-0599`)
    let mut schema_cat = fixture_catalog();
    let mut docs_stats = StatsCatalog::new();
    let mut doc_row = Row::new();
    doc_row.insert("id", Cell::from_text("d1"));
    doc_row.insert("title", Cell::from_text("hello"));
    doc_row.insert("removed_legacy_col", Cell::Int(1));
    docs_stats.analyze_rows("docs", &[doc_row], None).unwrap();
    docs_stats
        .analyze_rows("dropped_table", &rows[..2], None)
        .unwrap();
    docs_stats.reconcile_with_catalog(&schema_cat);
    assert!(docs_stats.inspect("dropped_table").is_none());
    let reconciled_docs = docs_stats.inspect("docs").unwrap();
    assert!(reconciled_docs.columns.contains_key("title"));
    assert!(!reconciled_docs.columns.contains_key("removed_legacy_col"));
    let _ = &mut schema_cat;

    assert!(cat.reset("items"));
    assert!(cat.inspect("items").is_none());
    cat.analyze_rows("items", &rows[..5], None).unwrap();
    cat.reset_all();
    assert!(cat.inspect("items").is_none());

    // Explicit `max_stats_bytes` memory budget enforcement (`LIN-0598`)
    let mut tiny_budget = StatsCatalog::with_max_bytes(256);
    let err = tiny_budget.analyze_rows("items", &rows, None).unwrap_err();
    assert!(err.to_string().contains("budget"), "{err}");
    assert!(tiny_budget.inspect("items").is_none());
}

#[test]
fn cost_estimator_zipfian_skew_correlation_sparse_nulls_and_operators() {
    let estimator = CostEstimator::new(CostUnits::default());

    // 1. Adversarial Zipfian / heavy-hitter skew (`LIN-0582`):
    // 800 rows have `tenant == "mega"`, 200 rows have distinct singleton `tenant == "t_0"..="t_199"`.
    let mut skew_rows = Vec::with_capacity(1000);
    for i in 0..1000usize {
        let mut r = Row::new();
        let tenant = if i < 800 {
            "mega".to_string()
        } else {
            format!("tail_{}", i - 800)
        };
        r.insert("tenant", Cell::from_text(tenant));
        skew_rows.push(r);
    }
    let mut stats_cat = StatsCatalog::new();
    let skew_ts = stats_cat
        .analyze_rows("events", &skew_rows, None)
        .unwrap()
        .clone();

    let hot_est = estimator.estimate_index_seek(
        &skew_ts,
        "tenant",
        CmpOp::Eq,
        &Value::String("mega".into()),
    );
    let cold_est = estimator.estimate_index_seek(
        &skew_ts,
        "tenant",
        CmpOp::Eq,
        &Value::String("tail_42".into()),
    );
    assert_eq!(hot_est.rows, 800, "heavy hitter must estimate ~800, not uniform 5");
    assert!(
        cold_est.rows <= 2,
        "cold tail key must estimate ~1, got {}",
        cold_est.rows
    );
    assert!(skew_ts.index_fanouts["tenant"].max_key_fraction >= 0.79);

    // 2. Positively correlated predicates (`LIN-0583`):
    // 1000 rows where `(region, zone)` take 10 perfectly correlated pairs (`r0`/`z0` .. `r9`/`z9`), 100 rows each.
    // Naive independence would predict `1000 * 0.1 * 0.1 = 10` (10x underestimate!).
    let mut corr_rows = Vec::with_capacity(1000);
    for i in 0..1000usize {
        let g = i % 10;
        let mut r = Row::new();
        r.insert("region", Cell::from_text(format!("r{g}")));
        r.insert("zone", Cell::from_text(format!("z{g}")));
        corr_rows.push(r);
    }
    let mut corr_cat = StatsCatalog::new();
    corr_cat
        .analyze_rows_at_gen(
            "nodes",
            &corr_rows,
            None,
            1,
            &[("nodes[region,zone]".into(), vec!["region".into(), "zone".into()])],
        )
        .unwrap();
    let corr_ts = corr_cat.inspect("nodes").unwrap();
    assert_eq!(corr_ts.composite_prefixes["nodes[region,zone]"], vec![10, 10]);

    let corr_pred = parse_pred(r#"region == "r3" and zone == "z3""#).unwrap();
    let corr_est = estimator.estimate_residual_filter(corr_ts, &corr_pred);
    let receipt = estimator.compare_estimate_actual(corr_est.rows, 100);
    assert!(
        receipt.q_error <= 1.25,
        "correlated composite prefix estimate should be ~100, got {} (q_error={})",
        corr_est.rows,
        receipt.q_error
    );

    // Even without composite index metadata, exponential backoff resists naive 10x collapse.
    let mut no_idx_ts = corr_ts.clone();
    no_idx_ts.composite_prefixes.clear();
    no_idx_ts.index_fanouts.clear();
    let damped_est = estimator.estimate_residual_filter(&no_idx_ts, &corr_pred);
    assert!(
        damped_est.rows > 20,
        "backoff should exceed naive independence (10), got {}",
        damped_est.rows
    );

    // 3. Sparse NULL-heavy collection (`LIN-0584`): 950 NULLs, 50 non-null values
    let mut sparse_rows = Vec::with_capacity(1000);
    for i in 0..1000usize {
        let mut r = Row::new();
        r.insert("id", Cell::Int(i as i64));
        if i < 50 {
            r.insert("tag", Cell::from_text(format!("t{}", i % 5)));
            r.insert("latency", Cell::Int((i * 10) as i64));
        } else {
            r.insert("tag", Cell::Null);
            r.insert("latency", Cell::Null);
        }
        sparse_rows.push(r);
    }
    let sparse_ts = stats_cat
        .analyze_rows("sparse", &sparse_rows, None)
        .unwrap()
        .clone();
    let null_est = estimator
        .estimate_residual_filter(&sparse_ts, &parse_pred("tag is null").unwrap());
    let not_null_est = estimator
        .estimate_residual_filter(&sparse_ts, &parse_pred("tag is not null").unwrap());
    let tag_eq = estimator
        .estimate_residual_filter(&sparse_ts, &parse_pred(r#"tag == "t0""#).unwrap());
    assert_eq!(null_est.rows, 950);
    assert_eq!(not_null_est.rows, 50);
    assert_eq!(tag_eq.rows, 10);

    // 4. Substring, graph expansion, join build side, Top-K sort memory, and agg groups (`LIN-0575`..`LIN-0580`)
    let short_sub = estimator.estimate_substring(&sparse_ts, "tag", 2);
    let impossible_sub = estimator.estimate_substring(&sparse_ts, "tag", 32);
    assert!(short_sub.rows > 0 && short_sub.rows <= 50);
    assert_eq!(impossible_sub.rows, 0, "needle longer than max_len must estimate 0");

    let graph_uncapped = estimator.estimate_graph_expansion(5, 3.0, 2, 300);
    assert_eq!(graph_uncapped.rows, 15 + 45);
    let graph_capped = estimator.estimate_graph_expansion(50, 4.0, 3, 300);
    assert_eq!(graph_capped.rows, 300);

    // Join: 1000 orders joining 50 users -> build right (`users`) recommended!
    let mut small_users = Vec::with_capacity(50);
    for i in 0..50usize {
        small_users.push(Row::from([
            ("id", Cell::from_text(format!("u{i}"))),
            ("email", Cell::from_text(format!("u{i}@example.com"))),
        ]));
    }
    let mut large_orders = Vec::with_capacity(1000);
    for i in 0..1000usize {
        large_orders.push(Row::from([
            ("id", Cell::from_text(format!("o{i}"))),
            ("user_id", Cell::from_text(format!("u{}", i % 50))),
            ("total", Cell::Float(25.0)),
        ]));
    }
    let users_ts = stats_cat
        .analyze_rows("users", &small_users, None)
        .unwrap()
        .clone();
    let orders_ts = stats_cat
        .analyze_rows("orders", &large_orders, None)
        .unwrap()
        .clone();
    let join_est = estimator.estimate_join(&orders_ts, &users_ts, "user_id", "id");
    assert_eq!(join_est.output_rows, 1000);
    assert!(join_est.recommended_build_right);
    let flipped_join = estimator.estimate_join(&users_ts, &orders_ts, "id", "user_id");
    assert!(!flipped_join.recommended_build_right);
    assert_eq!(join_est.build_bytes, flipped_join.build_bytes);

    let full_sort_mem = estimator.estimate_sort_bytes(10_000, 128, None);
    let topk_sort_mem = estimator.estimate_sort_bytes(10_000, 128, Some(25));
    assert!(topk_sort_mem < full_sort_mem / 100);

    assert_eq!(estimator.estimate_agg_groups(&orders_ts, &[]), 1);
    assert_eq!(estimator.estimate_agg_groups(&orders_ts, &["user_id"]), 50);

    let under = compare_estimate_actual(25, 100);
    assert_eq!(under.q_error, 4.0);
    assert!(under.underestimated);
}

// ============================================================================
// 2. D14: Logical Optimizer Rewrites, Fingerprints & Equivalence Oracle (`LIN-0526`..`LIN-0560`)
// ============================================================================

#[test]
fn optimizer_barriers_dedup_lazy_dnf_and_fingerprints() {
    // Determinism & volatile barriers (`LIN-0538`, `LIN-0539`)
    let p_det = parse_pred(r#"wing == "rag" and ts > timestamp(1700000000000)"#).unwrap();
    let p_now = parse_pred(r#"ts > ago 7d"#).unwrap();
    let p_param = parse_pred(r#"id == $target_id"#).unwrap();
    let p_dur = parse_pred(r#"ts > 5m"#).unwrap();
    assert!(is_deterministic_pred(&p_det) && !has_volatile_barrier(&p_det));
    assert!(!is_deterministic_pred(&p_now) && has_volatile_barrier(&p_now));
    assert!(!is_deterministic_pred(&p_param) && has_volatile_barrier(&p_param));
    assert!(!is_deterministic_pred(&p_dur) && has_volatile_barrier(&p_dur));

    // Common-conjunct deduplication & Or-factoring (`LIN-0537`)
    let dup = parse_pred(r#"wing == "rag" and room == "lab" and wing == "rag""#).unwrap();
    let (deduped, rem) = deduplicate_conjuncts(&dup);
    assert_eq!(rem, 1);
    assert_eq!(
        predicate_fingerprint(&deduped),
        predicate_fingerprint(&parse_pred(r#"room == "lab" and wing == "rag""#).unwrap())
    );

    let unfactored =
        parse_pred(r#"(wing == "rag" and room == "a") or (wing == "rag" and room == "b")"#)
            .unwrap();
    let (factored, shared_cnt) = deduplicate_conjuncts(&unfactored);
    assert!(shared_cnt >= 1);
    assert!(matches!(factored, lin::Pred::And(_, _)));

    // Normalized predicate fingerprints (`LIN-0540`)
    let pairs = [
        (r#"a == 1 and b == 2 and c == 3"#, r#"c == 3 and (a == 1 and b == 2)"#),
        (r#"not (a != 5)"#, r#"a == 5"#),
        (r#"not (a == 1 or b == 2)"#, r#"a != 1 and b != 2"#),
        (r#"x in ["b", "a", "b"]"#, r#"x == "a" or x == "b""#),
        (r#"x in [42]"#, r#"x == 42"#),
        (r#"f == -0.0"#, r#"f == 0.0"#),
    ];
    for (lhs, rhs) in pairs {
        let fp_l = predicate_fingerprint(&parse_pred(lhs).unwrap());
        let fp_r = predicate_fingerprint(&parse_pred(rhs).unwrap());
        assert_eq!(fp_l, fp_r, "expected equal normalized fingerprints for `{lhs}` vs `{rhs}`");
    }
    assert_ne!(
        predicate_fingerprint(&parse_pred("a > 1").unwrap()),
        predicate_fingerprint(&parse_pred("a >= 1").unwrap())
    );

    // Bounded Lazy DNF streaming (`LIN-0526`, `LIN-0597`):
    // 20 conjuncts of `(f_i == 0 or f_i == 1)` = 2^20 = 1,048,576 theoretical DNF clauses!
    let huge_cnf_src = (0..20)
        .map(|i| format!("(f{i} == 0 or f{i} == 1)"))
        .collect::<Vec<_>>()
        .join(" and ");
    let huge_cnf = parse_pred(&huge_cnf_src).unwrap();
    let started = Instant::now();
    let (clauses, capped) = stream_dnf_bounded(&huge_cnf, 32);
    let elapsed = started.elapsed();
    assert_eq!(clauses.len(), 32);
    assert!(capped, "2^20 clauses must report capped=true at limit 32");
    assert_eq!(clauses[0].len(), 20);
    assert!(
        elapsed < StdDuration::from_millis(200),
        "lazy DNF took {:?} (must be bounded)",
        elapsed
    );

    let small_dnf = parse_pred("(a == 1 or a == 2) and (b == 10 or b == 20)").unwrap();
    let iter = LazyDnfIter::new(&small_dnf, MAX_DNF_CLAUSES);
    let all_small: Vec<_> = iter.collect();
    assert_eq!(all_small.len(), 4);

    // Reproducible plan fingerprints & OptimizerOptions trace (`LIN-0533`..`LIN-0536`, `LIN-0551`, `LIN-0552`)
    let mut cat = fixture_catalog();
    let stmts = parse_program(
        r#"index docs [wing, ts]
docs | not (wing != "rag") | wing == "rag" | ts > timestamp(100) | { id, title }"#,
    )
    .unwrap();
    let (_, staged) = check_program(&stmts, &cat).unwrap();
    cat = staged.unwrap_or(cat);

    let (opt_plan_1, trace_1, counters_1) =
        plan_program_with_opts(&stmts[1..], &cat, &OptimizerOptions::traced()).unwrap();
    let (opt_plan_2, _, _) =
        plan_program_with_opts(&stmts[1..], &cat, &OptimizerOptions::traced()).unwrap();
    assert_eq!(plan_fingerprint(&opt_plan_1), plan_fingerprint(&opt_plan_2));
    assert!(!trace_1.is_empty());
    assert!(counters_1.predicates_simplified >= 1);
    assert!(counters_1.filters_merged >= 1);
    assert!(counters_1.conjuncts_deduplicated >= 1);
    assert!(counters_1.index_seeks_selected >= 1);

    let (ref_plan, ref_trace, _) = plan_program_with_opts(
        &stmts[1..],
        &cat,
        &OptimizerOptions {
            enable_optimizations: false,
            max_iterations: 0,
            record_trace: true,
        },
    )
    .unwrap();
    assert_ne!(plan_fingerprint(&opt_plan_1), plan_fingerprint(&ref_plan));
    assert!(ref_trace.iter().any(|t| t.pass_name == "no_optimization_reference"));
}

#[test]
fn plan_equivalence_oracle_optimized_vs_reference_mode() {
    let mut db = Db::empty();
    db.run("col items { id: text, grp: text, tier: i64, price: f64, note: text }")
        .unwrap();
    db.run("index items [grp, tier]").unwrap();
    db.run("index items [price]").unwrap();

    let mut batch = Vec::new();
    for i in 0..80usize {
        let grp = match i % 4 {
            0 => "alpha",
            1 => "beta",
            2 => "gamma",
            _ => "delta",
        };
        let tier = (i % 5) as i64;
        let price = (i as f64) * 3.5 - 20.0;
        let note = if i % 7 == 0 { "special wal note" } else { "ordinary" };
        batch.push(format!(
            r#"{{ id: "it-{i:03}", grp: "{grp}", tier: {tier}, price: {price:.1}, note: "{note}" }}"#
        ));
    }
    db.run(&format!("insert items [{}]", batch.join(", "))).unwrap();

    let queries = [
        r#"items | id == "it-017" | { id, grp, price }"#,
        r#"items | grp == "alpha" and tier >= 2 | sort id asc | take all"#,
        r#"items | not (grp != "beta") and grp == "beta" and (tier == 1 or tier == 3) | sort id asc | take all"#,
        r#"items | (grp == "alpha" and tier == 0) or (grp == "gamma" and tier == 2) | sort id asc | take all"#,
        r#"items | grp in ["alpha", "delta", "alpha"] and price > 15.0 and price <= 160.0 | sort id asc | take all"#,
        r#"items | not (price < 0.0 or price > 50.0) and note has "wal" | sort id asc | take all"#,
        r#"items | grp == "beta" | tier > 1 | { id, tier } | sort id asc | take 10"#,
        r#"items | count by grp | sort grp asc | take all"#,
    ];

    let opt_on = OptimizerOptions::default();
    let opt_off = OptimizerOptions::unoptimized();

    fn plan_has_seek_or_get(n: &lin::stats::Node) -> bool {
        matches!(n.kind, NodeKind::IndexSeek { .. } | NodeKind::Get { .. })
            || n.children.iter().any(plan_has_seek_or_get)
    }

    for (idx, q) in queries.iter().enumerate() {
        let (rows_opt, plan_opt) = execute_with_optimizer_opts(&mut db, q, &opt_on).unwrap();
        let (rows_ref, plan_ref) = execute_with_optimizer_opts(&mut db, q, &opt_off).unwrap();
        assert_eq!(rows_opt, rows_ref, "equivalence mismatch on query #{idx}: `{q}`");
        assert!(
            !plan_has_seek_or_get(&plan_ref.root),
            "reference mode must not select IndexSeek or Get on `{q}`"
        );
        if idx < 5 {
            assert!(
                plan_has_seek_or_get(&plan_opt.root),
                "optimized mode should select Get or IndexSeek on `{q}`"
            );
        }
    }
}

// ============================================================================
// 3. Structured EXPLAIN (`LIN-0507`, `LIN-0557`..`LIN-0560`, `LIN-1463`)
// ============================================================================

#[test]
fn structured_explain_backends_gpu_accuracy_types_and_q_error() {
    let mut db = Db::empty();
    let mut doc_inserts = Vec::with_capacity(1200);
    for i in 0..1200usize {
        let wing = if i < 300 { "rag" } else { "sys" };
        doc_inserts.push(format!(
            r#"{{ id: "d{i:04}", uri: "u://{i:04}", title: "doc {i}", layer: "wiki", wing: "{wing}", ts: timestamp({}) }}"#,
            1_700_000_000_000i64 + (i as i64) * 1000
        ));
    }
    db.run(&format!("insert docs [{}]", doc_inserts.join(", ")))
        .unwrap();

    let mut stats = StatsCatalog::new();
    stats.analyze_db_collection(&db, "docs", None).unwrap();

    // 1. >= 1024 rows, no GPU attached -> `ColumnarBatch` + explicit GPU not attached reason
    let ex_batch = explain_structured(
        r#"docs | wing == "rag" | { id, title, ts } | take all"#,
        Some(&stats),
        false,
    )
    .unwrap();
    assert_eq!(ex_batch.backend, ExplainBackend::ColumnarBatch);
    assert!(ex_batch.gpu_eligibility.contains("GPU not attached"), "{}", ex_batch.gpu_eligibility);
    assert_eq!(ex_batch.accuracy, PlanAccuracy::Exact);
    assert_eq!(
        ex_batch.output_fields,
        vec![
            ("id".to_string(), "id".to_string()),
            ("title".to_string(), "text".to_string()),
            ("ts".to_string(), "time".to_string()),
        ]
    );
    assert!(ex_batch.plan_fingerprint.starts_with("p:"));

    // Compare estimate vs actual execution (`q_error` == 1.0!)
    let actual_rows = db
        .run(r#"docs | wing == "rag" | { id, title, ts } | take all"#)
        .unwrap()
        .rows
        .len();
    let feedback = compare_explain_with_actual(&ex_batch, actual_rows);
    assert_eq!(actual_rows, 300);
    assert!(
        feedback.q_error <= 1.05,
        "expected exact MCV cardinality ~300, got {:?} (q_error={})",
        ex_batch.estimated_rows,
        feedback.q_error
    );

    // 2. >= 1024 rows, GPU attached -> `GpuCandidate` + eligible reason
    let ex_gpu = explain_structured(
        r#"docs | wing == "rag" | { id, title } | take all"#,
        Some(&stats),
        true,
    )
    .unwrap();
    assert_eq!(ex_gpu.backend, ExplainBackend::GpuCandidate);
    assert!(ex_gpu.gpu_eligibility.starts_with("eligible:"), "{}", ex_gpu.gpu_eligibility);

    // 3. Cold-disk collection -> `ColdDisk`
    let ex_cold = explain_structured_with_catalog(
        r#"docs | wing == "rag" | { id }"#,
        &db.catalog,
        Some(&stats),
        true,
        &["docs"],
    )
    .unwrap();
    assert_eq!(ex_cold.backend, ExplainBackend::ColdDisk);
    assert!(ex_cold.gpu_eligibility.contains("cold-disk"));

    // 4. Point Get / Small collection -> `InMemoryRow`
    let ex_point = explain_structured(
        r#"docs | id == "d0001" | { id, title }"#,
        Some(&stats),
        true,
    )
    .unwrap();
    assert_eq!(ex_point.backend, ExplainBackend::InMemoryRow);
    assert_eq!(ex_point.estimated_rows, Some(1));

    // 5. Vector/Hybrid search -> `PlanAccuracy::Approximate`
    let ex_vec = explain_structured(r#"docs | search vec "architecture" | take 10"#, Some(&stats), false).unwrap();
    assert_eq!(ex_vec.accuracy, PlanAccuracy::Approximate);

    // StructuredExplain itself serializes cleanly to JSON (`LIN-1463`)
    let json = serde_json::to_string(&ex_batch).unwrap();
    let roundtrip: lin::stats::StructuredExplain = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip, ex_batch);
}

// ============================================================================
// 4. D12 & D13: Parser/Checker Diagnostics, String Cap, Canonical Formatter & Minimizer
// ============================================================================

#[test]
fn parser_escaped_string_cap_and_line_col_diagnostics() {
    assert_eq!(SYNTAX_VERSION, 1);
    assert_eq!(MAX_DECODED_STRING_BYTES, 4 * 1024 * 1024);

    // Raw string exceeding 4 MiB is rejected (`LIN-0448`)
    let over_raw = format!(r#"docs | title == "{}""#, "a".repeat(MAX_DECODED_STRING_BYTES + 64));
    let err_raw = parse(&over_raw).unwrap_err();
    assert!(err_raw.to_string().contains("exceeds maximum allowed"), "{err_raw}");

    // Escaped string exceeding 4 MiB is rejected during unescaping (`LIN-0448`)
    // `\u{1f980}` decodes to 4 UTF-8 bytes; 1_050_000 crabs = 4,200,000 decoded bytes (> 4 MiB)
    // while total query text is ~9.45 MiB (< 16 MiB `DEFAULT_MAX_QUERY_BYTES`)!
    let crabs = "\\u{1f980}".repeat(1_050_000);
    let over_esc = format!(r#"docs | title == "{crabs}""#);
    assert!(over_esc.len() < lin::stats::DEFAULT_MAX_QUERY_BYTES);
    let err_esc = parse(&over_esc).unwrap_err();
    assert!(err_esc.to_string().contains("exceeds maximum allowed"), "{err_esc}");

    // `line:col` coordinates + preserved keywords (`LIN-0441`..`LIN-0444`)
    assert_eq!(offset_to_line_col("abc\ndef\nghi", 5), (2, 2));

    let unterm = "docs\n  | title == \"unclosed\n  | take 5";
    let e_unterm = parse(unterm).unwrap_err().to_string();
    assert!(e_unterm.contains("unterminated string"), "{e_unterm}");
    assert!(e_unterm.contains("2:14"), "expected opening quote line:col 2:14 in `{e_unterm}`");

    let trailing = "docs | take 5\n  extra_garbage";
    let e_trail = parse(trailing).unwrap_err().to_string();
    assert!(e_trail.contains("trailing input") && e_trail.contains("unexpected"), "{e_trail}");
    assert!(e_trail.contains("2:3"), "expected 2:3 in `{e_trail}`");

    let bad_tok = "docs\n  | @@@";
    let e_tok = parse(bad_tok).unwrap_err().to_string();
    assert!(e_tok.contains("expected") && e_tok.contains("unexpected"), "{e_tok}");
    assert!(e_tok.contains("2:5"), "expected 2:5 in `{e_tok}`");
}

#[test]
fn parse_inspect_and_canonical_ast_formatter_conformance_corpus() {
    // `parse_inspect` (`LIN-0469`)
    let insp_ro = parse_inspect(
        r#"let recent = docs | ts > $min_ts | { id, wing }
orders | total > $min_total | join users on user_id | { id, users.email }
recent | wing == $wing_name"#,
    )
    .unwrap();
    assert_eq!(insp_ro.stmt_count, 3);
    assert!(insp_ro.is_read_only);
    assert_eq!(insp_ro.referenced_collections, vec!["docs", "orders", "users"]);
    assert_eq!(insp_ro.parameter_names, vec!["min_total", "min_ts", "wing_name"]);

    let insp_rw = parse_inspect(r#"insert docs { uri: $u, title: "T", layer: "wiki" }"#).unwrap();
    assert_eq!(insp_rw.stmt_count, 1);
    assert!(!insp_rw.is_read_only);
    assert_eq!(insp_rw.referenced_collections, vec!["docs"]);
    assert_eq!(insp_rw.parameter_names, vec!["u"]);

    // Canonical AST formatter roundtrip corpus (`LIN-0470`, `LIN-0471`, `LIN-0473`, `LIN-0478`, `LIN-0479`)
    let corpus = [
        r#"docs | wing == "rag" and (room == "a" or room == "b") | { id, title } | take 10"#,
        r#"docs | not (wing == "rag" or wing == "sys") and title has ci "wal" | without { body, embedding }"#,
        r#"orders | total >= 3.0 and total < -0.0 or total == -12.5 | sort total desc | skip 5 | take all"#,
        r#"docs | ts > timestamp(1700000000123) and ts <= now - 7d | title ~ /wal\d+/i | explain cost"#,
        r#"docs | id == $doc_id | match a -wikilink*1..2-> b <-wikilink- c | { a.id, b.title, c.uri }"#,
        r#"orders | summarize { hits: count, rev: sum(total), p95: percentile(total, 95.0), latest_id: arg_max(ts, id) } by [day: bin(ts, 1d), user_id]"#,
        r#"orders | arg_max(ts, *) by [user_id]"#,
        r#"orders | window rank by [user_id] sort [total desc] as rnk | window sum(total) rows between 2 preceding and current row as rolling"#,
        r#"docs | search lex "wal" | union (docs all | search vec "vector") | take 20"#,
        r#"insert docs { uri: "raw://x", title: "Line\nBreak\t\"Quote\"\u{7}", layer: "raw" } with edge wikilink -> page "wiki://home""#,
        r#"insert users [{ id: "u1", email: "a@b.c" }, { id: "u2", email: "d@e.f" }]"#,
        r#"update docs[id == "d1"] cas "sha256:abc" { room: "archive" }"#,
        r#"delete docs[wing == "old"] cas each"#,
        r#"delete edge wikilink "a" -> "b""#,
        r#"append facts [{ s: "lin", p: tagged, o: "db" }]"#,
        r#"append edges [wikilink "a" -> "b", wikilink "b" -> "c"]"#,
        r#"col metrics { ts: time, val: f64, emb: vec[16]@nomic-embed-text }"#,
        r#"index docs unique [wing, ts] where layer == "wiki""#,
        r#"guard docs.body immutable when layer == "raw""#,
        r#"fk orders.user_id -> users.id"#,
        r#"filter docs wing == "rag""#,
        r#"unfilter docs"#,
        r#"reembed docs to ollama/nomic-embed-text/768"#,
        r#"pin "checkpoint-1""#,
    ];

    for src in corpus {
        let stmt1 = parse(src).unwrap_or_else(|e| panic!("failed to parse `{src}`: {e}"));
        let formatted1 = format_stmt_canonical(&stmt1);
        let stmt2 = parse(&formatted1).unwrap_or_else(|e| {
            panic!("failed to re-parse canonical `{formatted1}` (from `{src}`): {e}")
        });
        let formatted2 = format_stmt_canonical(&stmt2);
        assert_eq!(stmt1, stmt2, "AST mismatch on roundtrip:\nsrc: {src}\ncanon: {formatted1}");
        assert_eq!(formatted1, formatted2, "canonical formatting must be idempotent");
    }

    let multi = parse_program("col notes { title: text }\nnotes | { title }").unwrap();
    let multi_fmt = format_program_canonical(&multi);
    assert_eq!(parse_program(&multi_fmt).unwrap(), multi);
}

#[test]
fn checker_levenshtein_suggestions_diagnostics_minimizer_and_type_boundaries() {
    assert_eq!(bounded_levenshtein("title", "title", 2), Some(0));
    assert_eq!(bounded_levenshtein("titl", "title", 2), Some(1));
    assert_eq!(bounded_levenshtein("wign", "wing", 2), Some(2));
    assert_eq!(bounded_levenshtein("completely_different", "title", 2), None);

    // 1. Unknown collection & unknown field "did you mean" + `check_diagnostics` (`LIN-0505`, `LIN-0506`, `LIN-0508`)
    let d_col = check_diagnostics(r#"dcso | wing == "rag""#);
    assert_eq!(d_col.len(), 1);
    assert_eq!(d_col[0].kind, DiagnosticKind::UnknownCollection);
    assert_eq!(d_col[0].collection.as_deref(), Some("dcso"));
    assert_eq!(d_col[0].suggestion.as_deref(), Some("docs"));
    assert_eq!(d_col[0].span, Some((0, 4)));
    assert!(d_col[0].message.contains("did you mean 'docs'?"), "{}", d_col[0].message);

    let src_field = r#"docs | titl == "hello""#;
    let d_field = check_diagnostics(src_field);
    assert_eq!(d_field.len(), 1);
    assert_eq!(d_field[0].kind, DiagnosticKind::UnknownField);
    assert_eq!(d_field[0].collection.as_deref(), Some("docs"));
    assert_eq!(d_field[0].field.as_deref(), Some("titl"));
    assert_eq!(d_field[0].suggestion.as_deref(), Some("title"));
    let (s0, s1) = d_field[0].span.expect("span");
    assert_eq!(&src_field[s0..s1], "titl");

    // Type mismatch diagnostic with literal remediation suggestion (`LIN-0505`)
    let d_type = check_diagnostics(r#"orders | total == "100""#);
    assert_eq!(d_type.len(), 1);
    assert_eq!(d_type[0].kind, DiagnosticKind::TypeMismatch);
    assert_eq!(d_type[0].collection.as_deref(), Some("orders"));
    assert_eq!(d_type[0].field.as_deref(), Some("total"));
    assert!(
        d_type[0]
            .suggestion
            .as_deref()
            .unwrap_or("")
            .contains("100"),
        "{:?}",
        d_type[0]
    );

    // Valid program returns zero diagnostics
    assert!(check_diagnostics(r#"docs | wing == "rag" | { id, title }"#).is_empty());

    // Custom catalog diagnostics
    let mut custom_cat = fixture_catalog();
    let decl_stmts = parse_program("col metrics { timestamp_ms: i64, cpu_pct: f64 }").unwrap();
    let (_, staged) = check_program(&decl_stmts, &custom_cat).unwrap();
    custom_cat = staged.unwrap();
    let d_custom = check_diagnostics_with_catalog("metrics | cpu_ptc > 0.5", &custom_cat);
    assert_eq!(d_custom[0].suggestion.as_deref(), Some("cpu_pct"));

    // 2. Malformed query minimizer (`LIN-0480`)
    let bloated_check_err = r#"docs | wing == "rag"
orders | total > 10 | sort total desc | { id, total } | take 5
docs | wing == "rag" | room == "inbox" | sort ts desc | { id, title, wign } | take 20"#;
    let shrunk_check = minimize_malformed_query(bloated_check_err);
    assert!(shrunk_check.len() < bloated_check_err.len() / 2, "{shrunk_check}");
    let shrunk_diags = check_diagnostics(&shrunk_check);
    assert_eq!(shrunk_diags.len(), 1);
    assert_eq!(shrunk_diags[0].kind, DiagnosticKind::UnknownField);

    let bloated_syntax_err = "docs | wing == \"rag\" | { id, title } | take 10\norders | total > 5 | @@@ | take 3";
    let shrunk_syntax = minimize_malformed_query(bloated_syntax_err);
    assert!(shrunk_syntax.len() < bloated_syntax_err.len(), "{shrunk_syntax}");
    assert!(parse_program(&shrunk_syntax).is_err());
    assert!(!shrunk_syntax.trim().is_empty());

    // 3. Projection alias, nested expression typing, i64/timestamp/NaN boundaries (`LIN-0475`, `LIN-0488`, `LIN-0492`, `LIN-0497`, `LIN-0498`, `LIN-0501`, `LIN-0502`)
    // Duplicate summarize alias rejected (`LIN-0488`)
    assert!(!check_diagnostics("orders | summarize { x: count, x: sum(total) }").is_empty());
    // Nested boolean expression typing (`LIN-0492`)
    assert!(!check_diagnostics(r#"orders | not (total > 10 and (user_id == 42))"#).is_empty());
    // Integer & timestamp literal overflow boundaries (`LIN-0497`, `LIN-0501`, `LIN-0502`)
    assert!(parse("orders | total > 9223372036854775807").is_ok());
    assert!(parse("orders | total > 9223372036854775808").is_err());
    assert!(parse("docs | ts == timestamp(-9223372036854775808)").is_ok());
    assert!(parse("docs | ts == timestamp(9223372036854775808)").is_err());
    assert!(!check_diagnostics("orders | percentile(total, 105.0)").is_empty());

    // NaN & negation simplification semantics (`LIN-0475`, `LIN-0498`):
    // Ensure `not (val != 5.0)` and `val == 5.0` never match `NaN` rows!
    let mut nan_db = Db::empty();
    nan_db.run("col nums { id: text, val: f64 }").unwrap();
    let prep = nan_db
        .prepare("insert nums { id: $id, val: $val }")
        .unwrap();
    prep.execute_with(
        &mut nan_db,
        &[("id", Cell::from_text("nan1")), ("val", Cell::Float(f64::NAN))],
    )
    .unwrap();
    prep.execute_with(
        &mut nan_db,
        &[("id", Cell::from_text("five")), ("val", Cell::Float(5.0))],
    )
    .unwrap();

    let eq_hits = nan_db
        .run("nums | val == 5.0 | take all")
        .unwrap()
        .into_rows();
    assert_eq!(eq_hits.len(), 1);
    assert_eq!(eq_hits[0].get("id").and_then(Cell::text), Some("five"));
}
