//! Qualification suite for Secondary Indexes (`D10`–`D11`), FTS & Lexical Search (`D23`–`D25`),
//! Embeddings & HNSW (`D26`–`D28`), Graph Traversals (`D31`), Cold-Tier Cache Residency (`D32`),
//! and WAL Replication & Promotion Fencing (`D35`–`D36`).

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rustc_hash::FxHashSet;

use lin::catalog::IndexDef;
use lin::embed::{
    DistanceMetric, Embedder, HashingEmbedder, HnswConfig, HnswIndex, SafeEmbedderWrapper,
    explain_vector_similarity, inspect_row_embed_text,
};
use lin::error::{Error, ErrorCode};
use lin::exec::graph_ops::{
    GraphPathBudget, shortest_unweighted_path, shortest_weighted_path,
};
use lin::exec::search::{explain_lex_score, match_phrase_positions};
use lin::fts::{
    FtsIndex, TOKENIZER_VERSION, TokenizerConfig, inspect_tokens, intersect_postings_galloping,
};
use lin::index::{IndexKey, IndexPart, LiveIndex, PostingList};
use lin::ship::{
    HandshakeFailureCategory, ReplicationTelemetry, ShipWalHub, classify_handshake_failure,
    verify_promotion_fence,
};
use lin::{Cell, Db, HotCacheConfig, HotTierCache, Row};

// ---------------------------------------------------------------------------
// 1. LiveIndex: fanout stats, consistency inspector, missing-pos remove, rebuild
// ---------------------------------------------------------------------------

#[test]
fn live_index_fanout_consistency_and_cancellable_rebuild() {
    let def = IndexDef {
        collection: "docs".into(),
        fields: vec!["wing".into()],
        unique: false,
        pred: None,
        bitmap_card_field: None,
    };

    let mut rows: Vec<Row> = Vec::new();
    for i in 0..100usize {
        let wing = match i % 10 {
            0..=4 => "alpha",
            5..=7 => "beta",
            8 => "gamma",
            _ => "delta",
        };
        rows.push(Row::from([
            ("id", Cell::from_text(&format!("d-{i}"))),
            ("wing", Cell::from_text(wing)),
        ]));
    }

    let mut progress_steps = Vec::new();
    let cancelled = AtomicBool::new(false);
    let mut idx = LiveIndex::rebuild_with_progress(
        &def,
        &rows,
        &mut |done, total| progress_steps.push((done, total)),
        &|| cancelled.load(Ordering::Relaxed),
    )
    .expect("rebuild should succeed");

    assert!(!progress_steps.is_empty());
    assert_eq!(progress_steps.last().copied(), Some((100, 100)));
    for w in progress_steps.windows(2) {
        assert!(w[0].0 <= w[1].0 && w[0].1 == 100);
    }

    let fanout = idx.fanout_stats();
    assert_eq!(fanout.distinct_keys, 4);
    assert_eq!(fanout.total_postings, 100);
    assert_eq!(fanout.max_posting_len, 50);
    assert!((fanout.avg_fanout - 25.0).abs() < 1e-9);
    assert_eq!(fanout.p95_posting_len, 50);
    assert!((fanout.selectivity_estimate - 0.25).abs() < 1e-9);

    let rep0 = idx.inspect_consistency(rows.len());
    assert!(rep0.valid, "{rep0:?}");
    assert_eq!(rep0.forward_postings, 100);
    assert_eq!(rep0.reverse_entries, 100);

    // Removing a missing position must be a safe no-op (`LIN-0368`).
    idx.remove_at(999_999);
    idx.remove_row(
        999_999,
        &Row::from([("id", Cell::from_text("ghost")), ("wing", Cell::from_text("alpha"))]),
    );
    assert!(idx.inspect_consistency(rows.len()).valid);

    // Live mutations: update, swap_remove, insert_at.
    let updated = Row::from([("id", Cell::from_text("d-3")), ("wing", Cell::from_text("omega"))]);
    idx.update_row(3, &rows[3], &updated).unwrap();
    rows[3] = updated;
    assert!(idx.inspect_consistency(rows.len()).valid);

    let tail = rows.pop().unwrap();
    let doomed = std::mem::replace(&mut rows[10], tail.clone());
    idx.swap_remove_rows(10, &doomed, 99, &tail);
    let rep_mut = idx.inspect_consistency(rows.len());
    assert!(rep_mut.valid, "{rep_mut:?}");
    assert_eq!(rep_mut.forward_postings, 99);

    // Deliberate invariant corruptions must be caught by `inspect_consistency`.
    let mut bad_unsorted = idx.clone();
    let custom_key = IndexKey(smallvec::smallvec![IndexPart::Int(42)]);
    Arc::make_mut(&mut bad_unsorted.forward).insert(
        custom_key.clone(),
        PostingList::Standard(smallvec::smallvec![20, 5]),
    );
    let r_bad = bad_unsorted.inspect_consistency(rows.len());
    assert!(!r_bad.valid);
    assert!(r_bad.unsorted_lists >= 1);
    assert!(r_bad.forward_reverse_mismatches >= 1);

    let mut bad_oob_dup = idx.clone();
    Arc::make_mut(&mut bad_oob_dup.forward).insert(
        custom_key,
        PostingList::Standard(smallvec::smallvec![0, 0, 500]),
    );
    let r_oob = bad_oob_dup.inspect_consistency(rows.len());
    assert!(!r_oob.valid);
    assert!(r_oob.duplicate_positions >= 1);
    assert!(r_oob.out_of_bounds_positions >= 1);

    // Cooperative cancellation mid-rebuild (`LIN-0396`).
    let cancel_flag = AtomicBool::new(false);
    let err = LiveIndex::rebuild_with_progress(
        &def,
        &rows,
        &mut |done, _total| {
            if done >= 10 {
                cancel_flag.store(true, Ordering::Relaxed);
            }
        },
        &|| cancel_flag.load(Ordering::Relaxed),
    )
    .expect_err("rebuild must cancel cooperatively");
    assert_eq!(err.code(), ErrorCode::Canceled);
}

// ---------------------------------------------------------------------------
// 2. FtsIndex & Lexical Search: inspect_tokens, invariants, diff, galloping, phrase, explain
// ---------------------------------------------------------------------------

#[test]
fn fts_tokenizer_v2_combining_marks_sigma_stopwords_stemming_and_caps() {
    assert_eq!(TOKENIZER_VERSION, 2);

    let mut stopwords = FxHashSet::default();
    stopwords.insert("the".to_string());
    stopwords.insert("and".to_string());

    let cfg = TokenizerConfig {
        min_token_bytes: 2,
        max_token_bytes: 24,
        max_tokens_per_doc: 5,
        stopwords: Some(stopwords),
        ascii_stem_s_ing_ed: true,
    };

    // Combining acute mark `e\u{0301}`, Greek final capital sigma `ΟΔΟΣ` -> `οδος`,
    // stopwords `the`/`and`, ASCII stems `running`/`indexed`/`cats`, plus cap at 5 tokens.
    let text = "The cafe\u{0301} ΟΔΟΣ and running indexed cats extra_ignored";
    let spans = inspect_tokens(text, &cfg);
    assert_eq!(spans.len(), 5);

    assert_eq!(spans[0].token, "cafe\u{0301}");
    assert_eq!(&text[spans[0].byte_start..spans[0].byte_end], "cafe\u{0301}");
    assert_eq!(spans[0].position, 0);

    assert_eq!(spans[1].token, "οδος");
    assert!(spans[1].token.ends_with('ς'));
    assert_eq!(&text[spans[1].byte_start..spans[1].byte_end], "ΟΔΟΣ");

    assert_eq!(spans[2].token, "run");
    assert_eq!(spans[3].token, "index");
    assert_eq!(spans[4].token, "cat");
}

#[test]
fn fts_churn_invariants_diff_limits_gallop_phrase_and_explain() {
    let fields = vec!["title".to_string(), "body".to_string()];
    let mut rows: Vec<Row> = (0..80usize)
        .map(|i| {
            Row::from([
                ("id", Cell::from_text(&format!("doc-{i}"))),
                (
                    "title",
                    Cell::from_text(&format!("Title {i} {}", if i % 4 == 0 { "wal" } else { "mem" })),
                ),
                (
                    "body",
                    Cell::from_text(&format!("storage engine row_{i} ΟΔΟΣ shared")),
                ),
            ])
        })
        .collect();

    // Budgeted build: succeeds with generous cap, fails with tiny cap.
    let mut build_steps = 0usize;
    let mut fts = FtsIndex::build_with_limits(&fields, &rows, 1024 * 1024, &mut |done, total| {
        assert!(done <= total);
        build_steps += 1;
    })
    .expect("build within vocab limit");
    assert!(build_steps >= 1);

    let limit_err = FtsIndex::build_with_limits(&fields, &rows, 8, &mut |_, _| {})
        .expect_err("tiny vocab cap must fail");
    assert_eq!(limit_err.code(), ErrorCode::ResourceLimit);

    // Churn: updates (`sync_row`), swap-removes (`swap_remove_row`), and appends.
    for i in (0..40usize).step_by(3) {
        let updated = Row::from([
            ("id", Cell::from_text(&format!("doc-{i}"))),
            ("title", Cell::from_text("Updated wal header")),
            ("body", Cell::from_text(&format!("fresh_tok_{i} shared"))),
        ]);
        fts.sync_row(i, &rows[i], &updated);
        rows[i] = updated;
    }
    for victim in [7usize, 19, 31] {
        let last_idx = rows.len() - 1;
        let tail = rows.pop().unwrap();
        let doomed = std::mem::replace(&mut rows[victim], tail.clone());
        fts.swap_remove_row(victim, &doomed, last_idx, &tail);
    }
    let appended = Row::from([
        ("id", Cell::from_text("doc-extra")),
        ("title", Cell::from_text("Extra tail document")),
        ("body", Cell::from_text("galloping intersection target")),
    ]);
    let new_pos = rows.len();
    rows.push(appended.clone());
    fts.insert_row(new_pos, &appended);

    let vstats = fts.vocabulary_stats();
    assert!(vstats.unique_terms >= 20);
    assert!(vstats.vocabulary_bytes >= vstats.unique_terms);
    assert!(vstats.max_posting_len >= 50);

    let inv_before_fold = fts.check_invariants(Some(rows.len()));
    assert!(inv_before_fold.valid, "{inv_before_fold:?}");
    let diff_before_fold = fts.diff_against_rebuild(&rows);
    assert!(diff_before_fold.identical, "{diff_before_fold:?}");

    fts.fold_all();
    assert_eq!(fts.vocabulary_stats().compaction_debt_entries, 0);
    assert!(fts.check_invariants(Some(rows.len())).valid);
    assert!(fts.diff_against_rebuild(&rows).identical);

    // Galloping intersection vs naive BTreeSet intersection across skewed lists.
    let list_a: Vec<usize> = (0..10_000).step_by(2).collect();
    let list_b: Vec<usize> = vec![4, 102, 999, 2048, 4096, 9998];
    let list_c: Vec<usize> = (0..10_000).step_by(4).collect();
    let got = intersect_postings_galloping(&[&list_a, &list_b, &list_c]);
    let set_a: BTreeSet<usize> = list_a.iter().copied().collect();
    let set_b: BTreeSet<usize> = list_b.iter().copied().collect();
    let set_c: BTreeSet<usize> = list_c.iter().copied().collect();
    let expected: Vec<usize> = set_a
        .intersection(&set_b)
        .copied()
        .filter(|x| set_c.contains(x))
        .collect();
    assert_eq!(got, expected);
    assert!(intersect_postings_galloping(&[&list_a, &[]]).is_empty());

    // Phrase & proximity matching (`max_slop`).
    let sample = "Rust builds a fast durable transactional engine; engine fast mode";
    let exact = match_phrase_positions(sample, &["fast", "durable", "transactional"], 0);
    assert_eq!(exact.len(), 1);
    assert_eq!(&sample[exact[0].0..exact[0].1], "fast durable transactional");
    assert!(match_phrase_positions(sample, &["fast", "transactional"], 0).is_empty());
    let slop1 = match_phrase_positions(sample, &["fast", "transactional"], 1);
    assert_eq!(slop1.len(), 1);
    assert_eq!(&sample[slop1[0].0..slop1[0].1], "fast durable transactional");
    // Transposition `"fast engine"` matches `"engine fast"` at slop >= 2.
    assert!(!match_phrase_positions("engine fast", &["fast", "engine"], 2).is_empty());

    // Field-boosted BM25 lexical explanation & highlights.
    let doc_title_hit = Row::from([
        ("title", Cell::from_text("Durable WAL replication")),
        ("body", Cell::from_text("general overview notes")),
    ]);
    let doc_body_hit = Row::from([
        ("title", Cell::from_text("General overview notes")),
        ("body", Cell::from_text("durable wal replication")),
    ]);
    let weights = &[("title", 4.0), ("body", 1.0)];
    let ex_title = explain_lex_score(&doc_title_hit, weights, "durable wal");
    let ex_body = explain_lex_score(&doc_body_hit, weights, "durable wal");
    assert!(ex_title.total_score > ex_body.total_score);
    assert_eq!(ex_title.matched_terms.len(), 2);
    let sum_terms: f64 = ex_title.matched_terms.iter().map(|(_, s)| *s).sum();
    assert!((sum_terms - ex_title.total_score).abs() < 1e-9);
    assert_eq!(ex_title.highlight_spans.len(), 2);
    assert_eq!(
        &"Durable WAL replication"[ex_title.highlight_spans[0].0..ex_title.highlight_spans[0].1],
        "Durable"
    );
}

// ---------------------------------------------------------------------------
// 3. Embeddings & HNSW: seed determinism, connectivity, metrics, SafeEmbedderWrapper
// ---------------------------------------------------------------------------

#[test]
fn hnsw_config_reproducibility_connectivity_metrics_and_safe_embedder() {
    let dim = 32;
    let emb = HashingEmbedder::new("qual/32", dim);
    let cfg = HnswConfig {
        m: 12,
        ef_construction: 48,
        ef_search: 32,
        seed: 0xDEAD_BEEF_CAFE_BABEu64,
        max_visited_nodes: None,
    };

    let mut idx_a = HnswIndex::with_config(dim, cfg);
    let mut idx_b = HnswIndex::with_config(dim, cfg);
    for i in 0..96usize {
        let v = emb.embed(&format!("vector document topic_{} item_{i}", i % 7));
        idx_a.insert(i, Arc::clone(&v));
        idx_b.insert(i, v);
    }

    let conn = idx_a.inspect_connectivity();
    assert_eq!(conn.total_nodes, 96);
    assert_eq!(conn.reachable_from_entry, 96);
    assert_eq!(conn.disconnected_nodes, 0);

    let qv = emb.embed("vector document topic_3 item_17");
    let (hits_a, stats_a) = idx_a.search_with_stats(&qv, 5, None, None);
    let (hits_b, stats_b) = idx_b.search_with_stats(&qv, 5, None, None);
    assert_eq!(hits_a, hits_b, "identical seed must yield identical ANN results");
    assert_eq!(stats_a, stats_b);
    assert!(stats_a.visited_nodes >= 5 && stats_a.distance_evals >= stats_a.visited_nodes);

    let (_capped_hits, capped_stats) = idx_a.search_with_stats(&qv, 5, Some(64), Some(10));
    assert!(capped_stats.visited_nodes <= 10);

    // Distance metrics & near-tie corpus + similarity explanation.
    let q = [1.0f32, 0.0, 0.0, 0.0];
    let near_a = [0.999f32, 0.0447, 0.0, 0.0];
    let near_b = [0.998f32, 0.0632, 0.0, 0.0];
    let scaled = [3.0f32, 0.0, 0.0, 0.0];

    assert!(DistanceMetric::Cosine.score(&q, &near_a) > DistanceMetric::Cosine.score(&q, &near_b));
    assert!(DistanceMetric::DotProduct.score(&q, &scaled) > DistanceMetric::DotProduct.score(&q, &q));
    assert!(
        DistanceMetric::EuclideanL2.score(&q, &near_a)
            > DistanceMetric::EuclideanL2.score(&q, &scaled)
    );

    let v_expl = explain_vector_similarity(&[0.6, 0.8, 0.0], &[0.6, 0.8, 0.0], DistanceMetric::Cosine);
    assert!((v_expl.score - 1.0).abs() < 1e-5);
    assert!((v_expl.query_norm - 1.0).abs() < 1e-5);
    assert_eq!(v_expl.top_contributing_dims[0].0, 1);

    // Row embed text inspection.
    let inspect = inspect_row_embed_text(&Row::from([
        ("title", Cell::from_text("Hello")),
        ("body", Cell::from_text("brave new world")),
    ]));
    assert_eq!(inspect.assembled_text, "Hello brave new world");
    assert_eq!(inspect.byte_len, 21);
    assert_eq!(inspect.token_count, 4);

    // SafeEmbedderWrapper: panic catch, oversized text, malformed batch len, bad dim.
    struct AdversarialEmbedder {
        mode: &'static str,
    }
    impl Embedder for AdversarialEmbedder {
        fn id(&self) -> &str {
            "adv/4"
        }
        fn dim(&self) -> usize {
            4
        }
        fn embed(&self, text: &str) -> Arc<[f32]> {
            match self.mode {
                "panic" => panic!("boom on {text}"),
                "bad_dim" => Arc::from([1.0f32, 2.0, 3.0]),
                _ => Arc::from([0.5f32, 0.5, 0.5, 0.5]),
            }
        }
        fn embed_batch(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
            match self.mode {
                "panic" => panic!("batch boom"),
                "short_batch" => vec![Arc::from([1.0f32, 0.0, 0.0, 0.0]); texts.len().saturating_sub(1)],
                "bad_dim" => vec![Arc::from([1.0f32, 2.0]); texts.len()],
                _ => texts.iter().map(|t| self.embed(t)).collect(),
            }
        }
    }

    let safe_panic = SafeEmbedderWrapper::new(AdversarialEmbedder { mode: "panic" }, 64, 1024);
    assert!(safe_panic.try_embed("hi").is_err());
    assert!(safe_panic.try_embed_batch(&["a", "b"]).is_err());
    // Infallible Embedder trait fallback never panics and preserves exact dim & batch length.
    assert_eq!(safe_panic.embed("hi").len(), 4);
    assert_eq!(safe_panic.embed_batch(&["a", "b"]).len(), 2);

    let safe_short =
        SafeEmbedderWrapper::new(AdversarialEmbedder { mode: "short_batch" }, 64, 1024);
    assert!(safe_short.try_embed_batch(&["a", "b", "c"]).is_err());

    let safe_dim = SafeEmbedderWrapper::new(AdversarialEmbedder { mode: "bad_dim" }, 64, 1024);
    assert_eq!(
        safe_dim.try_embed("ok").unwrap_err().code(),
        ErrorCode::IncompatibleModel
    );

    let safe_ok = SafeEmbedderWrapper::new(AdversarialEmbedder { mode: "ok" }, 8, 32);
    assert_eq!(
        safe_ok.try_embed("input_exceeding_8_bytes").unwrap_err().code(),
        ErrorCode::ResourceLimit
    );
    assert_eq!(
        safe_ok
            .try_embed_batch(&["a", "b", "c"])
            .unwrap_err()
            .code(),
        ErrorCode::ResourceLimit
    );
}

// ---------------------------------------------------------------------------
// 4. Graph Traversals, Degree Stats & Consistency Checker
// ---------------------------------------------------------------------------

#[test]
fn graph_shortest_paths_filters_cycles_budgets_and_consistency() {
    let mut db = Db::empty();
    for id in ["A", "B", "C", "D", "X", "ISLAND"] {
        db.run(&format!(
            r#"insert docs {{ id: "{id}", uri: "raw://{id}", title: "Node {id}", layer: "wiki" }}"#
        ))
        .unwrap();
    }
    // Cyclic graph with fast unweighted shortcut A->D (high weight 10.0) vs A->B->C->D (weight 3.0)
    // plus cycle C->A and self-loop B->B and blocked node X.
    for (from, to) in [
        ("A", "D"),
        ("A", "B"),
        ("B", "B"),
        ("B", "C"),
        ("C", "A"),
        ("C", "D"),
        ("A", "X"),
        ("X", "D"),
    ] {
        db.store.append_edge_parts("links", from, to);
    }

    let deg = db.graph_degree_stats(Some("links"));
    assert_eq!(deg.edge_count, 8);
    assert_eq!(deg.distinct_sources, 4);
    assert_eq!(deg.distinct_targets, 5);
    assert_eq!(deg.max_out_degree, 3);

    let cons_ok = db.inspect_graph_consistency();
    assert!(cons_ok.valid, "{cons_ok:?}");
    assert_eq!(cons_ok.dangling_sources, 0);
    assert_eq!(cons_ok.dangling_targets, 0);

    // Unweighted BFS chooses 1-hop A -> D; blocking edge (A, D) & node X chooses A -> B -> C -> D.
    let bfs_direct = db
        .shortest_unweighted_path(
            "links",
            "A",
            "D",
            GraphPathBudget::default(),
            &|_| true,
            &|_, _| true,
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        bfs_direct.path,
        vec![Cell::from_text("A"), Cell::from_text("D")]
    );

    let bfs_filtered = shortest_unweighted_path(
        &db.store,
        "links",
        "A",
        "D",
        false,
        GraphPathBudget::default(),
        &|n| n != "X",
        &|u, v| !(u == "A" && v == "D"),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        bfs_filtered.path,
        vec![
            Cell::from_text("A"),
            Cell::from_text("B"),
            Cell::from_text("C"),
            Cell::from_text("D"),
        ]
    );

    // Weighted Dijkstra chooses A -> B -> C -> D (cost 3.0) over direct A -> D (cost 10.0).
    let (cost, w_expl) = shortest_weighted_path(
        &db.store,
        "links",
        "A",
        "D",
        false,
        GraphPathBudget::default(),
        &|n| n != "X",
        &|u, v| match (u, v) {
            ("A", "D") => Some(10.0),
            _ => Some(1.0),
        },
    )
    .unwrap()
    .unwrap();
    assert!((cost - 3.0).abs() < 1e-9);
    assert_eq!(
        w_expl.path,
        vec![
            Cell::from_text("A"),
            Cell::from_text("B"),
            Cell::from_text("C"),
            Cell::from_text("D"),
        ]
    );

    // Disconnected target terminates cleanly on cyclic graph with `Ok(None)`.
    let disconnected = db
        .shortest_unweighted_path(
            "links",
            "A",
            "ISLAND",
            GraphPathBudget::default(),
            &|_| true,
            &|_, _| true,
        )
        .unwrap();
    assert!(disconnected.is_none());

    // Frontier byte cap triggers `ErrorCode::ResourceLimit`.
    let tight_err = shortest_unweighted_path(
        &db.store,
        "links",
        "A",
        "ISLAND",
        false,
        GraphPathBudget {
            max_depth: 16,
            max_frontier_nodes: 16,
            max_frontier_bytes: 40,
        },
        &|_| true,
        &|_, _| true,
    )
    .expect_err("tight frontier byte budget must fail");
    assert_eq!(tight_err.code(), ErrorCode::ResourceLimit);

    // Dangling target edge is flagged by `inspect_graph_consistency`.
    db.store.append_edge_parts("links", "A", "MISSING_NODE");
    let cons_dangling = db.inspect_graph_consistency();
    assert!(!cons_dangling.valid);
    assert_eq!(cons_dangling.dangling_targets, 1);
}

// ---------------------------------------------------------------------------
// 5. Cold-Tier Cache (`HotTierCache`): residency report & eviction breakdown
// ---------------------------------------------------------------------------

#[test]
fn hot_tier_cache_residency_report_and_eviction_reasons() {
    let cache = HotTierCache::new(HotCacheConfig {
        enabled: true,
        max_bytes: 1600,
        max_rows: 3,
        max_vec_queries: 8,
    });

    for i in 0..6usize {
        let row = Arc::new(Row::from([
            ("id", Cell::from_text(&format!("k-{i}"))),
            ("title", Cell::from_text(&format!("Cached row number {i}"))),
        ]));
        cache.put("docs", &format!("k-{i}"), row);
    }

    let rep1 = cache.residency_report();
    assert!(rep1.resident_entries <= 3);
    assert!(rep1.resident_bytes <= rep1.budget_bytes);
    assert!(rep1.utilization_ratio > 0.0 && rep1.utilization_ratio <= 1.0);
    assert!(rep1.evictions_by_capacity >= 3);
    assert!(rep1.estimated_working_set_bytes > rep1.resident_bytes);

    // Invalidate one present key and one table segment.
    cache.invalidate("docs", "k-5");
    let rep2 = cache.residency_report();
    assert_eq!(rep2.evictions_by_invalidation, 1);

    let purged = cache.purge_and_inspect();
    assert_eq!(purged.resident_entries, 0);
    assert_eq!(purged.resident_bytes, 0);
    assert_eq!(purged.utilization_ratio, 0.0);
    assert!(purged.evictions_by_purge >= 1);
}

// ---------------------------------------------------------------------------
// 6. Replication Handshake Classification, Telemetry & Promotion Fencing
// ---------------------------------------------------------------------------

#[test]
fn ship_handshake_classification_telemetry_and_promotion_fencing() {
    assert_eq!(
        HandshakeFailureCategory::inspect_magic(*b"LIN\x06"),
        Ok(6)
    );
    assert_eq!(
        HandshakeFailureCategory::inspect_magic(*b"LIN\x07"),
        Ok(7)
    );
    assert_eq!(
        HandshakeFailureCategory::inspect_magic(*b"LIN\x05"),
        Err(HandshakeFailureCategory::UnsupportedProtocolVersion)
    );
    assert_eq!(
        HandshakeFailureCategory::inspect_magic(*b"HTTP"),
        Err(HandshakeFailureCategory::InvalidMagic)
    );

    // Live TCP wire test verifying `UnsupportedProtocolVersion` vs `InvalidMagic`.
    let dir = std::env::temp_dir().join(format!(
        "lin-qual-ship-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut primary = Db::open(&dir).unwrap();
    primary
        .run(r#"insert docs { id: "s1", uri: "raw://s1", title: "Ship", layer: "wiki" }"#)
        .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let srv = std::thread::spawn(move || {
        let e1 = lin::ship::serve_one(&primary, &listener).unwrap_err();
        let e2 = lin::ship::serve_one(&primary, &listener).unwrap_err();
        let _ = primary.close();
        (e1, e2)
    });

    {
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(b"LIN\x99\x00\x00").unwrap();
        let mut tmp = [0u8; 4];
        let _ = s.read(&mut tmp);
    }
    {
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(b"NOPE\x00\x00").unwrap();
        let mut tmp = [0u8; 4];
        let _ = s.read(&mut tmp);
    }

    let (err_ver, err_magic) = srv.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        classify_handshake_failure(&err_ver),
        Some(HandshakeFailureCategory::UnsupportedProtocolVersion)
    );
    assert_eq!(
        classify_handshake_failure(&err_magic),
        Some(HandshakeFailureCategory::InvalidMagic)
    );
    assert_eq!(
        classify_handshake_failure(&Error::runtime("wal ship: bad token")),
        Some(HandshakeFailureCategory::AuthRejected)
    );
    assert_eq!(
        classify_handshake_failure(&Error::runtime("wal ship: empty TLS CA")),
        Some(HandshakeFailureCategory::TlsError)
    );
    assert_eq!(
        classify_handshake_failure(&Error::deadline_exceeded("wal ship: socket timed out")),
        Some(HandshakeFailureCategory::IoTimeout)
    );
    assert_eq!(
        classify_handshake_failure(&Error::runtime(
            "wal ship: replica lag 50 exceeds max_lag_gens"
        )),
        Some(HandshakeFailureCategory::GenerationDiverged)
    );

    // ReplicationTelemetry & ShipWalHub counters.
    let hub = ShipWalHub::new();
    hub.record_lag(50 * 1024 * 1024, 120);
    hub.record_failure(&Error::runtime("wal ship: bad token"));
    hub.record_failure(&Error::deadline_exceeded("wal ship: timed out"));
    let telem = hub.telemetry();
    assert_eq!(telem.lag_bytes, 50 * 1024 * 1024);
    assert_eq!(telem.lag_ms, 120);
    assert_eq!(telem.reconnect_attempts, 2);
    assert_eq!(telem.auth_failures, 1);
    assert_eq!(telem.transport_failures, 1);
    let (min_eta, max_eta) = telem.catchup_eta_ms_bounds.expect("ETA bounds");
    assert!(min_eta > 0 && min_eta <= max_eta);
    assert_eq!(
        ReplicationTelemetry::compute_catchup_eta_bounds(0, 1000, 5000),
        Some((0, 0))
    );

    // Promotion fence verification (`LIN-1430`..`LIN-1432`).
    let receipt = verify_promotion_fence(42, 40, 7).expect("caught-up follower promotes");
    assert_eq!(receipt.promoted_epoch, 8);
    assert_eq!(receipt.fenced_generation, 42);
    assert!(receipt.previous_role_was_follower);

    let stale = verify_promotion_fence(39, 40, 7).expect_err("stale follower must be fenced");
    assert_eq!(stale.code(), ErrorCode::CasMismatch);
    assert_eq!(
        classify_handshake_failure(&stale),
        Some(HandshakeFailureCategory::GenerationDiverged)
    );
    assert!(verify_promotion_fence(10, 10, u64::MAX).is_err());
}
