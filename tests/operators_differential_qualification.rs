use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use lin::batch::{BatchAllocationBudget, Column, ColumnView, RecordBatch};
use lin::cursor::{active_cursor_count, dropped_unconsumed_cursor_count};
use lin::error::ErrorCode;
use lin::exec::agg::{GroupBudget, WelfordAcc, summarize_grouped_welford};
use lin::exec::join::{
    JoinAlgorithmKind, JoinBudget, anti_join_rows, hash_join_rows, join_cells_match,
    merge_join_sorted_rows, normalize_join_key, semi_join_rows,
};
use lin::persist::spill::{
    CollationPolicy, ExternalRowSorter, compare_cell_collated, compare_str_collated,
    merge_sorted_row_runs, top_k_rows_with_stats,
};
use lin::query::pred;
use lin::store::value::{
    RowView, acquire_general_rows, acquire_orders_row, purge_thread_row_pools,
    release_general_rows, release_orders_row, row_pool_stats,
};
use lin::window::{
    WindowEvalBudget, WindowFrameExclude, WindowFrameKind, evaluate_window_partition,
    percent_rank_values,
};
use lin::{Cell, Db, Row, Stmt, parse};

fn make_row<const N: usize>(pairs: [(&str, Cell); N]) -> Row {
    let mut r = Row::new();
    for (k, v) in pairs {
        r.insert(k, v);
    }
    r
}

#[test]
fn batch_slice_concat_reconcile_view_budget_consistency_and_arrow() {
    let long_s0: Arc<str> = Arc::from("shared_zero_copy_payload_string_0000");
    let long_s1: Arc<str> = Arc::from("shared_zero_copy_payload_string_0001");

    let mut c_int = Column::with_capacity(5);
    c_int.push(Cell::Int(10));
    c_int.push(Cell::Null);
    c_int.push(Cell::Int(30));
    c_int.push(Cell::Int(40));
    c_int.push(Cell::Null);

    let mut c_flt = Column::with_capacity(5);
    c_flt.push(Cell::Float(1.5));
    c_flt.push(Cell::Float(2.5));
    c_flt.push(Cell::Null);
    c_flt.push(Cell::Float(4.5));
    c_flt.push(Cell::Float(5.5));

    let mut c_bool = Column::with_capacity(5);
    for b in [Some(true), None, Some(false), Some(true), Some(false)] {
        c_bool.push(b.map_or(Cell::Null, Cell::Bool));
    }

    let mut c_time = Column::with_capacity(5);
    for t in [Some(1000), Some(2000), None, Some(4000), Some(5000)] {
        c_time.push(t.map_or(Cell::Null, Cell::Time));
    }

    let c_text = Column::from_strs([Some("alpha"), None, Some("beta"), Some("gamma"), None]);
    let c_dict = c_text.encode_dict().expect("dict encode");
    let c_mixed = Column::Cells(vec![
        Cell::Text(Arc::clone(&long_s0)),
        Cell::Int(7),
        Cell::Text(Arc::clone(&long_s1)),
        Cell::Null,
        Cell::Bool(true),
    ]);
    let c_nulls = Column::Nulls { n: 5, cap: 5 };

    let names: Arc<[String]> = vec![
        "ival".into(),
        "fval".into(),
        "bval".into(),
        "tval".into(),
        "txt".into(),
        "dict".into(),
        "mix".into(),
        "nulls".into(),
    ]
    .into();

    let batch = RecordBatch {
        names: Arc::clone(&names),
        cols: vec![c_int, c_flt, c_bool, c_time, c_text, c_dict, c_mixed, c_nulls],
    };
    batch.inspect_consistency().expect("batch is consistent");
    assert_eq!(batch.len(), 5);

    // Verify ColumnView across every representation.
    assert!(matches!(batch.cols[0].as_view(), ColumnView::Int(s, v) if s.len() == 5 && v.null_count() == 2));
    assert!(matches!(batch.cols[1].as_view(), ColumnView::Float(s, v) if s.len() == 5 && v.null_count() == 1));
    assert!(matches!(batch.cols[2].as_view(), ColumnView::Bool(s, v) if s.len() == 5 && v.null_count() == 1));
    assert!(matches!(batch.cols[3].as_view(), ColumnView::Time(s, v) if s.len() == 5 && v.null_count() == 1));
    match batch.cols[4].as_view() {
        ColumnView::Text { valid, .. } => {
            assert_eq!(valid.len(), 5);
            assert_eq!(valid.null_count(), 2);
            assert_eq!(batch.cols[4].as_view().text_at(2), Some("beta"));
        }
        other => panic!("expected Text view, got {other:?}"),
    }
    match batch.cols[5].as_view() {
        ColumnView::Dict { valid, .. } => {
            assert_eq!(valid.null_count(), 2);
            assert_eq!(batch.cols[5].as_view().text_at(3), Some("gamma"));
        }
        other => panic!("expected Dict view, got {other:?}"),
    }
    assert!(matches!(batch.cols[6].as_view(), ColumnView::Mixed(s) if s.len() == 5));
    assert!(matches!(batch.cols[7].as_view(), ColumnView::Nulls(5)));

    // Slice shares schema Arc and Cell::Text Arc pointers while recomputing exact null counts.
    let sliced = batch.slice(1..4);
    sliced.inspect_consistency().expect("sliced batch is consistent");
    assert!(Arc::ptr_eq(&sliced.names, &batch.names));
    assert_eq!(sliced.len(), 3);
    assert_eq!(sliced.to_rows(), batch.to_rows()[1..4]);
    assert_eq!(sliced.cols[0].null_count(), 1);
    if let (ColumnView::Mixed(orig), ColumnView::Mixed(sub)) =
        (batch.cols[6].as_view(), sliced.cols[6].as_view())
    {
        if let (Cell::Text(a), Cell::Text(b)) = (&orig[2], &sub[1]) {
            assert!(Arc::ptr_eq(a, b), "Arc<str> must be shared zero-copy on slice");
        } else {
            panic!("expected heap text cell");
        }
    }

    // Validity::slice_range across 64-bit word boundaries.
    let mut big_col = Column::with_capacity(200);
    for i in 0..200 {
        big_col.push(if i % 7 == 0 { Cell::Null } else { Cell::Int(i) });
    }
    let (_, big_valid) = big_col.as_i64().unwrap();
    let v_sub = big_valid.slice_range(61..133);
    assert_eq!(v_sub.len(), 72);
    let expected_nulls = (61..133).filter(|i| i % 7 == 0).count();
    assert_eq!(v_sub.null_count(), expected_nulls);
    for idx in 0..72 {
        assert_eq!(v_sub.is_valid(idx), (61 + idx) % 7 != 0);
    }

    // RecordBatch::concat matches row-level concatenation.
    let s_head = batch.slice(0..2);
    let s_tail = batch.slice(2..5);
    let joined = RecordBatch::concat(&[&s_head, &s_tail]).expect("concat succeeds");
    joined.inspect_consistency().expect("concat consistent");
    assert_eq!(joined.to_rows(), batch.to_rows());

    // RecordBatch::reconcile_schema aligns missing columns with nulls and promotes Int + Float -> Float.
    let b1 = RecordBatch::from_rows(&[
        make_row([("id", Cell::from_text("a")), ("score", Cell::Int(42))]),
        make_row([("id", Cell::from_text("b")), ("score", Cell::Int(7))]),
    ]);
    let b2 = RecordBatch::from_rows(&[make_row([
        ("id", Cell::from_text("c")),
        ("score", Cell::Float(12.5)),
        ("extra", Cell::Bool(true)),
    ])]);
    let reconciled = RecordBatch::reconcile_schema(&[b1, b2]).expect("reconcile succeeds");
    reconciled.inspect_consistency().expect("reconciled consistent");
    assert_eq!(reconciled.len(), 3);
    let score_idx = reconciled.names.iter().position(|n| n == "score").unwrap();
    assert!(
        matches!(reconciled.cols[score_idx], Column::Float { .. }),
        "Int + Float must promote cleanly to dense Column::Float"
    );
    let rec_rows = reconciled.to_rows();
    assert_eq!(rec_rows[0].get("score"), Some(&Cell::Float(42.0)));
    assert_eq!(rec_rows[0].get("extra"), Some(&Cell::Null));
    assert_eq!(rec_rows[2].get("extra"), Some(&Cell::Bool(true)));

    // Incompatible schema types are rejected cleanly.
    let b_bad_1 = RecordBatch::from_rows(&[make_row([("x", Cell::Int(1))])]);
    let b_bad_2 = RecordBatch::from_rows(&[make_row([("x", Cell::Bool(false))])]);
    assert!(RecordBatch::reconcile_schema(&[b_bad_1, b_bad_2]).is_err());

    // Budget & diagnostics & Arrow descriptors.
    let mem_diag = batch.memory_diagnostics();
    assert_eq!(mem_diag.len(), batch.cols.len());
    assert!(batch.estimated_bytes() > 0);
    assert!(
        batch
            .check_budget(&BatchAllocationBudget {
                max_rows: 10,
                max_bytes: 1_000_000,
            })
            .is_ok()
    );
    let err_rows = batch
        .check_budget(&BatchAllocationBudget {
            max_rows: 3,
            max_bytes: 1_000_000,
        })
        .unwrap_err();
    assert_eq!(err_rows.code(), ErrorCode::ResourceLimit);

    let arrow = batch.describe_arrow_schema();
    assert_eq!(arrow.format, "+s");
    assert_eq!(arrow.children.len(), batch.cols.len());
    assert!(!arrow.ownership_contract.is_empty());
    let exports = batch.export_arrow_descriptors();
    assert_eq!(exports.len(), batch.cols.len());
    assert!(exports.iter().all(|e| e.length == 5 && e.release_required));
}

#[test]
fn cursor_batched_fetch_size_hint_cancellation_token_and_leak_counter() {
    let mut db = Db::empty();
    let mut stmt = String::from("insert users [");
    for i in 0..25 {
        if i > 0 {
            stmt.push(',');
        }
        stmt.push_str(&format!(r#"{{ id: "u-{i:02}", email: "u{i}@lin.dev" }}"#));
    }
    stmt.push(']');
    db.run(&stmt).unwrap();

    let Stmt::Query(q) = parse("users | { id, email } | take 20").unwrap() else {
        unreachable!()
    };

    let active_before = active_cursor_count();
    let leaked_before = dropped_unconsumed_cursor_count();

    {
        let mut cur = db.cursor(&q).unwrap();
        assert_eq!(active_cursor_count(), active_before + 1);
        assert_eq!(cur.size_hint_rows(), (20, Some(20)));

        let b1 = cur.fetch_batch(7).unwrap();
        assert_eq!(b1.len(), 7);
        assert_eq!(cur.stats().consumed_rows, 7);
        assert_eq!(cur.size_hint_rows(), (13, Some(13)));

        let rb = cur.fetch_record_batch(5).unwrap();
        assert_eq!(rb.len(), 5);
        assert_eq!(cur.stats().consumed_rows, 12);

        let tok = cur.resume_token(b1.last());
        assert_eq!(tok.consumed_rows, 12);
        assert_eq!(tok.last_id, Some(Cell::from_text("u-06")));
        tok.validate_generation(db.stats().r#gen)
            .expect("matching generation passes");
        assert_eq!(
            tok.validate_generation(db.stats().r#gen + 1)
                .unwrap_err()
                .code(),
            ErrorCode::StalePrepared
        );

        let receipt = cur.cancel_with_reason("client disconnected");
        assert_eq!(receipt.consumed_rows, 12);
        assert_eq!(receipt.reason, "client disconnected");
        let st = cur.stats();
        assert!(st.cancelled);
        assert_eq!(st.retained_bytes, 0);
        assert!(cur.fetch_batch(10).unwrap().is_empty());
    }
    assert_eq!(active_cursor_count(), active_before);
    assert_eq!(
        dropped_unconsumed_cursor_count(),
        leaked_before,
        "explicitly cancelled cursor must not count as leaked"
    );

    // Dropping a partially consumed cursor without cancellation increments leak counter.
    {
        let mut leaked_cur = db.cursor(&q).unwrap();
        let _ = leaked_cur.fetch_batch(2).unwrap();
    }
    assert_eq!(dropped_unconsumed_cursor_count(), leaked_before + 1);

    // Generation invalidation after actual DB mutation.
    let mut cur_tok = db.cursor(&q).unwrap();
    let first = cur_tok.fetch_batch(1).unwrap();
    let tok = cur_tok.resume_token(first.first());
    let _ = cur_tok.cancel_with_reason("done");
    drop(cur_tok);
    db.run(r#"insert users [{ id: "u-99", email: "u99@lin.dev" }]"#)
        .unwrap();
    assert!(tok.validate_generation(db.stats().r#gen).is_err());
}

#[test]
fn welford_catastrophic_cancellation_nan_filter_budget_and_zipf_skew() {
    // 1. Catastrophic cancellation resistance: 10^9 + {1, 2, 3}.
    let shifted = [1_000_000_001.0, 1_000_000_002.0, 1_000_000_003.0];
    let mut acc = WelfordAcc::new();
    assert_eq!(acc.count(), 0);
    assert_eq!(acc.mean(), None);
    assert_eq!(acc.variance_pop(), None);
    assert_eq!(acc.variance_sample(), None);

    acc.update(shifted[0]);
    assert_eq!(acc.count(), 1);
    assert_eq!(acc.variance_pop(), Some(0.0));
    assert_eq!(acc.stddev_pop(), Some(0.0));
    assert_eq!(acc.variance_sample(), None);
    assert_eq!(acc.stddev_sample(), None);

    acc.update(shifted[1]);
    acc.update(shifted[2]);
    assert!((acc.mean().unwrap() - 1_000_000_002.0).abs() < 1e-9);
    assert!((acc.variance_pop().unwrap() - (2.0 / 3.0)).abs() < 1e-12);
    assert!((acc.variance_sample().unwrap() - 1.0).abs() < 1e-12);
    assert!((acc.stddev_sample().unwrap() - 1.0).abs() < 1e-12);

    // 2. Pairwise Chan merge against exact shifted two-pass oracle.
    let offsets: Vec<f64> = (0..200).map(|i| ((i * 17 % 31) as f64) * 0.25).collect();
    let values: Vec<f64> = offsets.iter().map(|&d| 1.0e8 + d).collect();
    let exact_off_mean = offsets.iter().sum::<f64>() / (offsets.len() as f64);
    let exact_mean = 1.0e8 + exact_off_mean;
    let exact_m2: f64 = offsets
        .iter()
        .map(|&off| {
            let d = off - exact_off_mean;
            d * d
        })
        .sum();
    let exact_var_pop = exact_m2 / (values.len() as f64);
    let exact_var_samp = exact_m2 / ((values.len() - 1) as f64);

    let mut left_acc = WelfordAcc::new();
    for &v in &values[..73] {
        left_acc.update(v);
    }
    let mut right_acc = WelfordAcc::new();
    for &v in &values[73..] {
        right_acc.update(v);
    }
    left_acc.merge(&right_acc);
    assert_eq!(left_acc.count(), values.len());
    assert!((left_acc.mean().unwrap() - exact_mean).abs() < 1e-12);
    assert!((left_acc.variance_pop().unwrap() - exact_var_pop).abs() < 1e-12);
    assert!((left_acc.variance_sample().unwrap() - exact_var_samp).abs() < 1e-12);

    // 3. NaN propagation.
    let mut nan_acc = WelfordAcc::new();
    nan_acc.update(10.0);
    nan_acc.update(f64::NAN);
    nan_acc.update(20.0);
    assert!(nan_acc.mean().unwrap().is_nan());
    assert!(nan_acc.variance_pop().unwrap().is_nan());
    assert!(nan_acc.variance_sample().unwrap().is_nan());
    assert!(nan_acc.stddev_pop().unwrap().is_nan());
    assert!(nan_acc.stddev_sample().unwrap().is_nan());

    // 4. Grouped Welford with FILTER, Zipf skew diagnostics, and GroupBudget overflow.
    let mut rows = Vec::new();
    // Heavy hitter group "hot" gets 80 active rows, "warm" gets 15, "cold" gets 5, plus 20 filtered-out rows.
    for i in 0..80 {
        rows.push(make_row([
            ("grp", Cell::from_text("hot")),
            ("active", Cell::Bool(true)),
            ("v", Cell::Float(100.0 + (i % 4) as f64)),
        ]));
    }
    for i in 0..15 {
        rows.push(make_row([
            ("grp", Cell::from_text("warm")),
            ("active", Cell::Bool(true)),
            ("v", Cell::Float(10.0 + (i % 3) as f64)),
        ]));
    }
    for i in 0..5 {
        rows.push(make_row([
            ("grp", Cell::from_text("cold")),
            ("active", Cell::Bool(true)),
            ("v", Cell::Int(i)),
        ]));
    }
    for _ in 0..20 {
        rows.push(make_row([
            ("grp", Cell::from_text("ignored")),
            ("active", Cell::Bool(false)),
            ("v", Cell::Float(9999.0)),
        ]));
    }

    let flt = pred::eq("active", true);
    let (grouped, skew) = summarize_grouped_welford(
        &rows,
        &["grp"],
        "v",
        Some(&flt),
        &GroupBudget {
            max_groups: 10,
            max_bytes: 64 * 1024,
        },
    )
    .expect("grouped welford succeeds");

    assert_eq!(grouped.len(), 3);
    assert_eq!(skew.total_rows, 100);
    assert_eq!(skew.distinct_groups, 3);
    assert_eq!(skew.max_group_rows, 80);
    assert!((skew.top_group_share - 0.80).abs() < 1e-12);
    assert!(!skew.overflowed);

    let overflow_err = summarize_grouped_welford(
        &rows,
        &["grp"],
        "v",
        Some(&flt),
        &GroupBudget {
            max_groups: 2,
            max_bytes: 64 * 1024,
        },
    )
    .unwrap_err();
    assert_eq!(overflow_err.code(), ErrorCode::ResourceLimit);
}

#[test]
fn collation_external_spill_merge_topk_and_temp_cleanup_on_cancel() {
    assert_eq!(
        compare_str_collated("item2", "item10", CollationPolicy::NaturalSort),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        compare_str_collated("release-1.9", "release-1.12", CollationPolicy::NaturalSort),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        compare_str_collated("Alpha", "alpha", CollationPolicy::AsciiCaseInsensitive),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        compare_str_collated("Alpha", "alpha", CollationPolicy::Binary),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        compare_cell_collated(
            &Cell::from_text("node9"),
            &Cell::from_text("node12"),
            CollationPolicy::NaturalSort
        ),
        std::cmp::Ordering::Less
    );

    let spill_dir = std::env::temp_dir().join(format!(
        "lin_qual_extsort_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&spill_dir).unwrap();

    let mut rows = Vec::with_capacity(320);
    for i in 0..320usize {
        let num = (i * 73) % 140;
        rows.push(make_row([
            ("id", Cell::Int(i as i64)),
            ("label", Cell::from_text(format!("item{num}"))),
            ("payload", Cell::from_text("x".repeat(48))),
        ]));
    }

    // Tiny memory budget forces multi-run spill to disk + k-way heap merge.
    let sorter = ExternalRowSorter::new(2048)
        .with_collation(CollationPolicy::NaturalSort)
        .with_temp_dir(&spill_dir);

    let (sorted, diag) = sorter
        .sort_rows_external(rows.clone(), "label", false, || false)
        .expect("external sort succeeds");
    assert_eq!(sorted.len(), rows.len());
    assert!(diag.spilled_runs >= 3, "expected >=3 spilled runs, got {}", diag.spilled_runs);
    assert!(diag.spilled_bytes > 0);
    assert!(diag.comparisons > 0);

    // Verify natural sort order AND stability on equal `label` keys (`id` strictly increasing).
    for w in sorted.windows(2) {
        let la = w[0].get("label").unwrap();
        let lb = w[1].get("label").unwrap();
        let ord = compare_cell_collated(la, lb, CollationPolicy::NaturalSort);
        assert!(ord != std::cmp::Ordering::Greater, "out of order: {la:?} vs {lb:?}");
        if ord == std::cmp::Ordering::Equal {
            assert!(w[0].get("id").unwrap().as_i64() < w[1].get("id").unwrap().as_i64());
        }
    }
    assert_eq!(
        std::fs::read_dir(&spill_dir).unwrap().count(),
        0,
        "all temporary spill files must be removed after merge"
    );

    // Cancellation during external sort must remove every temporary file.
    let calls = AtomicUsize::new(0);
    let cancel_err = sorter
        .sort_rows_external(rows.clone(), "label", false, || {
            calls.fetch_add(1, Ordering::Relaxed) >= 3
        })
        .unwrap_err();
    assert_eq!(cancel_err.code(), ErrorCode::Canceled);
    assert_eq!(
        std::fs::read_dir(&spill_dir).unwrap().count(),
        0,
        "temp spill files must be cleaned up on cancellation"
    );

    // Verify `merge_sorted_row_runs` and `top_k_rows_with_stats`.
    let r_a = vec![
        make_row([("k", Cell::from_text("item1"))]),
        make_row([("k", Cell::from_text("item10"))]),
    ];
    let r_b = vec![
        make_row([("k", Cell::from_text("item2"))]),
        make_row([("k", Cell::from_text("item20"))]),
    ];
    let (merged_runs, m_diag) =
        merge_sorted_row_runs(vec![r_a, r_b], "k", false, CollationPolicy::NaturalSort).unwrap();
    assert_eq!(m_diag.input_rows, 4);
    let merged_labels: Vec<&str> = merged_runs
        .iter()
        .map(|r| r.get("k").unwrap().text().unwrap())
        .collect();
    assert_eq!(merged_labels, vec!["item1", "item2", "item10", "item20"]);

    let (top5, tk_diag) =
        top_k_rows_with_stats(&rows, "label", false, 5, CollationPolicy::NaturalSort);
    assert_eq!(top5, &sorted[..5]);
    assert_eq!(tk_diag.topk_rejected, rows.len() - 5);

    let _ = std::fs::remove_dir_all(&spill_dir);
}

#[test]
fn join_operators_cross_numeric_keys_nan_semi_anti_and_budgets() {
    assert!(join_cells_match(&Cell::Int(42), &Cell::Float(42.0)));
    assert!(join_cells_match(&Cell::Int(0), &Cell::Float(-0.0)));
    assert!(!join_cells_match(
        &Cell::Float(f64::NAN),
        &Cell::Float(f64::NAN)
    ));
    assert!(!join_cells_match(&Cell::Null, &Cell::Null));
    // Above 2^53, rounded float must NOT falsely match distinct integer 2^53 + 1.
    let big_int = (1i64 << 53) + 1;
    assert!(!join_cells_match(
        &Cell::Int(big_int),
        &Cell::Float(big_int as f64)
    ));
    assert!(normalize_join_key(&Cell::Float(f64::NAN)).is_none());

    let left = vec![
        make_row([("lid", Cell::from_text("L0")), ("k", Cell::Null)]),
        make_row([("lid", Cell::from_text("L1")), ("k", Cell::Int(10))]),
        make_row([("lid", Cell::from_text("L2")), ("k", Cell::Float(42.0))]),
        make_row([("lid", Cell::from_text("L3")), ("k", Cell::Int(42))]),
        make_row([("lid", Cell::from_text("L4")), ("k", Cell::Int(99))]),
        make_row([("lid", Cell::from_text("L5")), ("k", Cell::Float(f64::NAN))]),
    ];
    let right = vec![
        make_row([("rid", Cell::from_text("R0")), ("k", Cell::Null)]),
        make_row([("rid", Cell::from_text("R1")), ("k", Cell::Float(10.0))]),
        make_row([("rid", Cell::from_text("R2")), ("k", Cell::Int(42))]),
        make_row([("rid", Cell::from_text("R3")), ("k", Cell::Float(42.0))]),
        make_row([("rid", Cell::from_text("R4")), ("k", Cell::Float(f64::NAN))]),
    ];

    let budget = JoinBudget {
        max_output_rows: 100,
        max_build_bytes: 1024 * 1024,
    };

    let (hash_out, hash_diag) =
        hash_join_rows(&left, &right, "k", "k", "r", &budget, || false).unwrap();
    let (merge_out, merge_diag) =
        merge_join_sorted_rows(&left, &right, "k", "k", "r", &budget, || false).unwrap();

    assert_eq!(hash_diag.algorithm, JoinAlgorithmKind::HashJoin);
    assert_eq!(merge_diag.algorithm, JoinAlgorithmKind::SortMergeJoin);
    // Expected matches: L1xR1 (1) + (L2,L3)x(R2,R3) (4) = 5 rows; Null and NaN never match!
    assert_eq!(hash_out.len(), 5);
    assert_eq!(hash_out, merge_out, "hash join and sort-merge join must agree");
    assert_eq!(merge_diag.hot_keys, vec![("42".to_string(), 2)]);

    let (semi_out, semi_diag) =
        semi_join_rows(&left, &right, "k", "k", &budget, || false).unwrap();
    assert_eq!(semi_diag.algorithm, JoinAlgorithmKind::HashSemiJoin);
    let semi_ids: Vec<&str> = semi_out
        .iter()
        .map(|r| r.get("lid").unwrap().text().unwrap())
        .collect();
    assert_eq!(semi_ids, vec!["L1", "L2", "L3"]);

    let (anti_out, anti_diag) =
        anti_join_rows(&left, &right, "k", "k", &budget, || false).unwrap();
    assert_eq!(anti_diag.algorithm, JoinAlgorithmKind::HashAntiJoin);
    let anti_ids: Vec<&str> = anti_out
        .iter()
        .map(|r| r.get("lid").unwrap().text().unwrap())
        .collect();
    assert_eq!(anti_ids, vec!["L0", "L4", "L5"]);

    // Budget enforcement & cancellation.
    let tight = JoinBudget {
        max_output_rows: 2,
        max_build_bytes: 1024 * 1024,
    };
    assert_eq!(
        merge_join_sorted_rows(&left, &right, "k", "k", "r", &tight, || false)
            .unwrap_err()
            .code(),
        ErrorCode::ResourceLimit
    );
    assert_eq!(
        semi_join_rows(&left, &right, "k", "k", &budget, || true)
            .unwrap_err()
            .code(),
        ErrorCode::Canceled
    );
}

#[test]
fn window_frames_rows_range_groups_exclude_and_percent_rank() {
    assert!(percent_rank_values(0, &[]).is_empty());
    assert_eq!(percent_rank_values(1, &[1]), vec![0.0]);
    assert_eq!(percent_rank_values(5, &[1, 2, 2, 4, 5]), vec![0.0, 0.25, 0.25, 0.75, 1.0]);

    let rows = vec![
        make_row([("ts", Cell::Time(100)), ("val", Cell::Float(10.0))]),
        make_row([("ts", Cell::Time(100)), ("val", Cell::Float(20.0))]),
        make_row([("ts", Cell::Time(105)), ("val", Cell::Float(30.0))]),
        make_row([("ts", Cell::Time(120)), ("val", Cell::Float(40.0))]),
    ];
    let budget = WindowEvalBudget {
        max_partition_rows: 64,
        max_bytes: 64 * 1024,
    };

    // 1. ROWS frame with CurrentRow exclusion.
    let (rows_excl, diag) = evaluate_window_partition(
        &rows,
        "ts",
        Some("val"),
        WindowFrameKind::Rows {
            preceding: Some(1),
            following: Some(1),
        },
        WindowFrameExclude::CurrentRow,
        &budget,
    )
    .unwrap();
    assert_eq!(diag.partitions, 1);
    assert_eq!(diag.frame_evaluations, 4);
    assert_eq!(rows_excl, vec![20.0, 40.0, 60.0, 30.0]);

    // 2. Timestamp RANGE frame (+/- 6 ms).
    let (range_vals, _) = evaluate_window_partition(
        &rows,
        "ts",
        Some("val"),
        WindowFrameKind::Range { max_delta_f64: 6.0 },
        WindowFrameExclude::NoOthers,
        &budget,
    )
    .unwrap();
    assert_eq!(range_vals, vec![60.0, 60.0, 60.0, 40.0]);

    // 3. Peer GROUPS frame (0 preceding peers, 1 following peer group) + Ties exclusion.
    let (group_vals, _) = evaluate_window_partition(
        &rows,
        "ts",
        Some("val"),
        WindowFrameKind::Groups {
            preceding_peers: 0,
            following_peers: 1,
        },
        WindowFrameExclude::NoOthers,
        &budget,
    )
    .unwrap();
    // Peer groups: {10, 20} (ts=100), {30} (ts=105), {40} (ts=120).
    assert_eq!(group_vals, vec![60.0, 60.0, 70.0, 40.0]);

    let (ties_vals, _) = evaluate_window_partition(
        &rows,
        "ts",
        Some("val"),
        WindowFrameKind::Groups {
            preceding_peers: 0,
            following_peers: 0,
        },
        WindowFrameExclude::Ties,
        &budget,
    )
    .unwrap();
    assert_eq!(ties_vals, vec![10.0, 20.0, 30.0, 40.0]);

    // Budget rejection.
    assert_eq!(
        evaluate_window_partition(
            &rows,
            "ts",
            Some("val"),
            WindowFrameKind::Rows {
                preceding: None,
                following: None,
            },
            WindowFrameExclude::NoOthers,
            &WindowEvalBudget {
                max_partition_rows: 2,
                max_bytes: 64 * 1024,
            },
        )
        .unwrap_err()
        .code(),
        ErrorCode::ResourceLimit
    );
}

#[test]
fn row_pool_stats_purge_and_zero_copy_row_view() {
    let _ = purge_thread_row_pools();
    let baseline = row_pool_stats();
    assert_eq!(baseline.retained_vecs, 0);
    assert_eq!(baseline.retained_rows, 0);
    assert_eq!(baseline.retained_bytes_approx, 0);

    // Miss on empty orders row pool, then release and hit!
    let r1 = acquire_orders_row();
    assert_eq!(row_pool_stats().misses, baseline.misses + 1);
    release_orders_row(r1);
    let after_release = row_pool_stats();
    assert_eq!(after_release.retained_rows, 1);
    assert!(after_release.retained_bytes_approx > 0);

    let r2 = acquire_orders_row();
    assert_eq!(row_pool_stats().hits, baseline.hits + 1);
    release_orders_row(r2);

    // General Vec<Row> pool (>= 1024 rows).
    let big_vec: Vec<Row> = (0..1024)
        .map(|i| make_row([("id", Cell::Int(i))]))
        .collect();
    release_general_rows(big_vec);
    let with_general = row_pool_stats();
    assert!(with_general.retained_rows >= 1025);
    let recycled = acquire_general_rows();
    assert!(recycled.is_some());
    release_general_rows(recycled.unwrap());

    let freed = purge_thread_row_pools();
    assert!(freed > 0);
    let purged = row_pool_stats();
    assert_eq!(purged.retained_vecs, 0);
    assert_eq!(purged.retained_rows, 0);
    assert_eq!(purged.retained_bytes_approx, 0);

    // Zero-allocation borrowed RowView.
    let heap_str: Arc<str> = Arc::from("zero_copy_borrowed_field_value_longer_than_inline");
    let row = make_row([
        ("active", Cell::Bool(true)),
        ("count", Cell::Int(99)),
        ("name", Cell::Text(Arc::clone(&heap_str))),
        ("ratio", Cell::Float(3.25)),
    ]);
    let view = RowView(&row);
    assert_eq!(view.len(), 4);
    let got_str = view.get_str("name").unwrap();
    assert_eq!(got_str.as_ptr(), heap_str.as_ptr());
    assert_eq!(view.get_i64("count"), Some(99));
    assert_eq!(view.get_f64("ratio"), Some(3.25));
    assert_eq!(view.get_f64("count"), Some(99.0));
    assert_eq!(view.get_bool("active"), Some(true));
    let field_names: Vec<&str> = view.iter_fields().map(|(k, _)| k).collect();
    assert_eq!(field_names, vec!["active", "count", "name", "ratio"]);
}

#[cfg(feature = "async")]
mod async_db_qualification {
    use std::time::Duration;

    use futures_util::StreamExt;
    use lin::async_db::{AsyncDb, AsyncExecutionLimits};
    use lin::error::ErrorCode;
    use lin::{Db, SyncMode};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn async_db_backpressure_queue_limit_timeout_and_checkpoint() {
        let dir = std::env::temp_dir().join(format!(
            "lin_async_qual_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let mut db = Db::open(&dir).unwrap().with_sync_mode(SyncMode::Normal);
        let mut insert_stmt = String::from("insert users [");
        for i in 0..80 {
            if i > 0 {
                insert_stmt.push(',');
            }
            insert_stmt.push_str(&format!(r#"{{ id: "u-{i:03}", email: "u{i}@lin.dev" }}"#));
        }
        insert_stmt.push(']');
        db.run(&insert_stmt).unwrap();

        // 1. Slow-consumer bounded stream (`max_stream_buffered_rows: 4`) + completion receipt.
        let adb = AsyncDb::new(db).with_limits(AsyncExecutionLimits {
            max_concurrent_blocking: 2,
            max_queued_queries: 8,
            max_queued_input_bytes: 256,
            max_stream_buffered_rows: 4,
            timeout: None,
        });

        let receipt = {
            let (stream, receipt_rx) = adb
                .stream_with_receipt("users | { id, email } | take 30")
                .await;
            futures_util::pin_mut!(stream);
            let mut count = 0usize;
            while let Some(item) = stream.next().await {
                let _row = item.unwrap();
                count += 1;
                if count == 5 {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            assert_eq!(count, 30);
            receipt_rx.await.unwrap()
        };
        assert_eq!(receipt.yielded_rows, 30);
        assert!(receipt.completed_cleanly);

        // Early stream drop updates `cancelled_by_drop`.
        {
            let s_drop = adb.stream("users | { id, email } | take 80").await;
            futures_util::pin_mut!(s_drop);
            let _first = s_drop.next().await.unwrap().unwrap();
        }
        for _ in 0..20 {
            if adb.telemetry_snapshot().cancelled_by_drop >= 1 {
                break;
            }
            tokio::task::yield_now().await;
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(adb.telemetry_snapshot().cancelled_by_drop >= 1);

        // 2. Input-byte queue limit rejection (`max_queued_input_bytes: 256`).
        let oversized_query = format!(
            r#"users | email != "{}" | take 1"#,
            "x".repeat(400)
        );
        let reject_err = adb.run_read(oversized_query).await.unwrap_err();
        assert_eq!(reject_err.code(), ErrorCode::ResourceLimit);
        assert!(adb.telemetry_snapshot().rejected_by_queue_limit >= 1);

        // 3. Concurrency + queue depth limit rejection when blocking slot is held.
        let tight_adb = adb.clone().with_limits(AsyncExecutionLimits {
            max_concurrent_blocking: 1,
            max_queued_queries: 0,
            max_queued_input_bytes: 64 * 1024,
            max_stream_buffered_rows: 2,
            timeout: None,
        });
        // Hold the single blocking permit via an open backpressured stream!
        {
            let held_stream = tight_adb.stream("users | take 50").await;
            futures_util::pin_mut!(held_stream);
            let _ = held_stream.next().await.unwrap().unwrap();
            let q_reject = tight_adb.run_read("users | take 1").await.unwrap_err();
            assert_eq!(q_reject.code(), ErrorCode::ResourceLimit);
            assert_eq!(tight_adb.telemetry_snapshot().rejected_by_queue_limit, 1);
        }

        // 4. Timeout cancellation (`timeout: Some(Duration::ZERO)` and contended slot timeout).
        let timeout_adb = adb.clone().with_limits(AsyncExecutionLimits {
            max_concurrent_blocking: 1,
            max_queued_queries: 4,
            max_queued_input_bytes: 64 * 1024,
            max_stream_buffered_rows: 2,
            timeout: Some(Duration::from_millis(15)),
        });
        {
            let blocker = timeout_adb.stream("users | take 50").await;
            futures_util::pin_mut!(blocker);
            let _ = blocker.next().await.unwrap().unwrap();
            let t_err = timeout_adb.run_read("users | take 1").await.unwrap_err();
            assert_eq!(t_err.code(), ErrorCode::DeadlineExceeded);
            assert!(timeout_adb.telemetry_snapshot().cancelled_by_timeout >= 1);
        }

        // 5. Durable `checkpoint_async` flushes & compacts WAL cleanly.
        adb.execute(r#"insert users [{ id: "u-999", email: "ckpt@lin.dev" }]"#)
            .await
            .unwrap();
        adb.checkpoint_async().await.unwrap();
        drop(adb);
        drop(tight_adb);
        drop(timeout_adb);

        let reopened = Db::open_read(&dir).unwrap();
        let check = reopened
            .run(r#"users | id == "u-999" | { email }"#)
            .unwrap();
        assert_eq!(check.rows.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
