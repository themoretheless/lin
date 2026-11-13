use super::index::MmapHashIndex;
use super::*;

#[test]
fn disk_roundtrip_point_reads_tombstones_and_vectors() {
    let tmp = std::env::temp_dir().join(format!("lin_disk_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);

    {
        let mut eng = DiskEngine::open(&tmp, false).unwrap();
        eng.commit_ops(vec![DiskWalOp::EnsureTable {
            schema: DiskTableSchema {
                name: "chunks".into(),
                columns: vec![
                    DiskColSchema {
                        name: "id".into(),
                        kind: DiskColKind::TextZstd,
                    },
                    DiskColSchema {
                        name: "document_id".into(),
                        kind: DiskColKind::TextZstd,
                    },
                    DiskColSchema {
                        name: "content".into(),
                        kind: DiskColKind::TextZstd,
                    },
                    DiskColSchema {
                        name: "chunk_index".into(),
                        kind: DiskColKind::Int64,
                    },
                    DiskColSchema {
                        name: "embedding".into(),
                        kind: DiskColKind::VecF32Sq8 { dim: 8 },
                    },
                ],
                indexed_fields: vec!["document_id".into()],
                cluster_by: None,
            },
        }])
        .unwrap();

        let mut r1 = Row::new();
        r1.insert("id".into(), Cell::from_text("c1"));
        r1.insert("document_id".into(), Cell::from_text("d1"));
        r1.insert("content".into(), Cell::from_text("hello disk backed world"));
        r1.insert("chunk_index".into(), Cell::Int(0));
        r1.insert(
            "embedding".into(),
            Cell::Vec(Arc::from([1.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])),
        );

        let mut r2 = Row::new();
        r2.insert("id".into(), Cell::from_text("c2"));
        r2.insert("document_id".into(), Cell::from_text("d1"));
        r2.insert("content".into(), Cell::from_text("second vector chunk"));
        r2.insert("chunk_index".into(), Cell::Int(1));
        r2.insert(
            "embedding".into(),
            Cell::Vec(Arc::from([0.0f32, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])),
        );

        eng.commit_ops(vec![DiskWalOp::UpsertBatch {
            table: "chunks".into(),
            rows: vec![r1, r2],
        }])
        .unwrap();
        eng.checkpoint().unwrap();
    }

    {
        let mut eng = DiskEngine::open(&tmp, false).unwrap();
        assert_eq!(eng.count_table("chunks"), 2);
        let c1 = eng.get_by_id("chunks", "c1").unwrap().unwrap();
        assert_eq!(
            c1.get("content").and_then(Cell::text),
            Some("hello disk backed world")
        );
        assert_eq!(eng.hot_cache_stats().row_misses, 1);
        let c1_hot = eng.get_by_id("chunks", "c1").unwrap().unwrap();
        assert_eq!(
            c1_hot.get("content").and_then(Cell::text),
            Some("hello disk backed world")
        );
        assert_eq!(eng.hot_cache_stats().row_hits, 1);

        let mut by_doc = Vec::new();
        eng.for_each_by_eq_projected("chunks", "document_id", &Cell::from_text("d1"), None, |r| {
            by_doc.push(r.clone());
            Ok(true)
        })
        .unwrap();
        assert_eq!(by_doc.len(), 2);

        let hits = eng
            .search_vectors_out_of_core(
                "chunks",
                "embedding",
                "document_id",
                &[0.95, 0.05, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                1,
                true,
                None,
                None,
                |_| true,
            )
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1.get("id").and_then(Cell::text), Some("c1"));

        let hits_hot = eng
            .search_vectors_out_of_core(
                "chunks",
                "embedding",
                "document_id",
                &[0.95, 0.05, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                1,
                true,
                None,
                None,
                |_| true,
            )
            .unwrap();
        assert_eq!(hits_hot.len(), 1);
        assert_eq!(eng.hot_cache_stats().vec_query_hits, 1);

        let hits_scoped = eng
            .search_vectors_out_of_core(
                "chunks",
                "embedding",
                "document_id",
                &[0.05, 0.95, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                1,
                false,
                Some(&["d1"]),
                None,
                |d| d == "d1",
            )
            .unwrap();
        assert_eq!(hits_scoped.len(), 1);
        assert_eq!(hits_scoped[0].1.get("id").and_then(Cell::text), Some("c2"));

        eng.begin_savepoint().unwrap();
        eng.commit_ops(vec![DiskWalOp::DeleteIds {
            table: "chunks".into(),
            ids: vec!["c1".into()],
        }])
        .unwrap();
        assert!(eng.get_by_id("chunks", "c1").unwrap().is_none());
        eng.rollback_savepoint();
        assert!(eng.get_by_id("chunks", "c1").unwrap().is_some());

        let bak_file = tmp.join("portable.lin");
        eng.export_portable_backup_file(&bak_file).unwrap();
        let migrated_dir =
            std::env::temp_dir().join(format!("lin_disk_mig_{}", std::process::id()));
        let mig_eng =
            DiskEngine::migrate_from_legacy(&bak_file, &migrated_dir, 1024, &BTreeMap::new())
                .unwrap();
        assert_eq!(mig_eng.count_table("chunks"), 2);
        let _ = fs::remove_dir_all(&migrated_dir);

        eng.commit_ops(vec![DiskWalOp::DeleteIds {
            table: "chunks".into(),
            ids: vec!["c1".into()],
        }])
        .unwrap();
        assert_eq!(eng.count_table("chunks"), 1);
        assert!(eng.get_by_id("chunks", "c1").unwrap().is_none());
    }

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn mmap_bm25_index_roundtrip_and_zero_heap_scoring() {
    let tmp = std::env::temp_dir().join(format!("lin_bm25_v3_test_{}.mmap", std::process::id()));
    let _ = fs::remove_file(&tmp);

    let mut builder = MmapBm25Builder::new(42);
    builder.add_document(
        "chunk-a",
        &[
            "rust".into(),
            "mmap".into(),
            "storage".into(),
            "mmap".into(),
        ],
    );
    builder.add_document("chunk-b", &["python".into(), "script".into()]);
    builder.add_document(
        "chunk-c",
        &["rust".into(), "vector".into(), "search".into()],
    );

    let idx = builder.finish_to_file(&tmp).unwrap();
    assert_eq!(idx.generation, 42);
    assert_eq!(idx.doc_count, 3);
    assert_eq!(idx.doc_id(0), Some("chunk-a"));
    assert_eq!(idx.doc_id(1), Some("chunk-b"));
    assert_eq!(idx.doc_id(2), Some("chunk-c"));
    assert_eq!(idx.doc_len(0), 4);

    assert!(MmapBm25Index::open_if_valid(&tmp, 99).is_none());
    let reopened = MmapBm25Index::open_if_valid(&tmp, 42).unwrap();
    let mut hits = reopened.score_terms(&["rust".into(), "mmap".into()], 1.2, 0.75);
    hits.sort_by(|a, b| b.1.total_cmp(&a.1));
    assert_eq!(hits.len(), 2);
    assert_eq!(reopened.doc_id(hits[0].0), Some("chunk-a"));
    assert_eq!(reopened.doc_id(hits[1].0), Some("chunk-c"));
    assert!(hits[0].1 > hits[1].1);

    let _ = fs::remove_file(&tmp);
}

#[test]
fn compaction_reclaims_tombstones_and_ivf_recalls_target_vector() {
    let tmp = std::env::temp_dir().join(format!("lin_disk_compact_ivf_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);

    let mut eng = DiskEngine::open(&tmp, false).unwrap();
    eng.commit_ops(vec![DiskWalOp::EnsureTable {
        schema: DiskTableSchema {
            name: "vecs".into(),
            columns: vec![
                DiskColSchema {
                    name: "id".into(),
                    kind: DiskColKind::TextZstd,
                },
                DiskColSchema {
                    name: "doc_id".into(),
                    kind: DiskColKind::TextZstd,
                },
                DiskColSchema {
                    name: "embedding".into(),
                    kind: DiskColKind::VecF32Sq8 { dim: 16 },
                },
            ],
            indexed_fields: vec!["doc_id".into()],
            cluster_by: None,
        },
    }])
    .unwrap();

    for b in 0..3usize {
        let mut rows = Vec::with_capacity(2_000);
        for i in 0..2_000usize {
            let gid = b * 2_000 + i;
            let mut v = [0.0f32; 16];
            let bucket = gid % 16;
            v[bucket] = 1.0;
            v[(bucket + 1) % 16] = ((gid % 97) as f32) * 0.005;
            let mut r = Row::new();
            r.insert("id".into(), Cell::from_text(format!("v-{gid}")));
            r.insert("doc_id".into(), Cell::from_text(format!("d-{}", gid % 50)));
            r.insert("embedding".into(), Cell::Vec(Arc::from(v)));
            rows.push(r);
        }
        eng.commit_ops(vec![DiskWalOp::UpsertBatch {
            table: "vecs".into(),
            rows,
        }])
        .unwrap();
        eng.checkpoint().unwrap();
    }
    assert_eq!(eng.segments["vecs"].len(), 3);

    let del_ids: Vec<String> = (0..1_000).map(|i| format!("v-{i}")).collect();
    eng.commit_ops(vec![DiskWalOp::DeleteIds {
        table: "vecs".into(),
        ids: del_ids,
    }])
    .unwrap();
    assert_eq!(eng.count_table("vecs"), 5_000);

    let rep = eng.vacuum().unwrap();
    assert_eq!(rep.tables_compacted, 1);
    assert_eq!(rep.old_segments_removed, 3);
    assert_eq!(rep.new_segments_written, 1);
    assert_eq!(rep.dead_rows_reclaimed, 1_000);
    assert_eq!(eng.count_table("vecs"), 5_000);
    assert!(eng.get_by_id("vecs", "v-10").unwrap().is_none());
    assert!(eng.get_by_id("vecs", "v-4500").unwrap().is_some());

    let seg = &eng.segments["vecs"][0];
    let vc = seg.vec_col("embedding").unwrap();
    assert!(
        vc.ivf.is_some(),
        "expected col_embedding.vivf on 5,000-row compacted segment"
    );
    let ivf = vc.ivf.as_ref().unwrap();
    assert_eq!(ivf.live_vecs, 5_000);

    let mut q = [0.0f32; 16];
    q[7] = 1.0;
    q[8] = 0.05;
    let qn = l2_norm_f64(&q);
    let top_c = ivf.top_centroids(&q, qn, 8);
    assert!(!top_c.is_empty());

    let hits = eng
        .search_vectors_out_of_core(
            "vecs",
            "embedding",
            "doc_id",
            &q,
            5,
            true,
            None,
            None,
            |_| true,
        )
        .unwrap();
    assert_eq!(hits.len(), 5);
    assert!(hits[0].0 > 0.99);

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn adaptive_text_dict_and_group_count_and_str3_visitor() {
    let tmp = std::env::temp_dir().join(format!("lin_disk_text_dict_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);

    let mut eng = DiskEngine::open(&tmp, false).unwrap();
    eng.commit_ops(vec![DiskWalOp::EnsureTable {
        schema: DiskTableSchema {
            name: "docs".into(),
            columns: vec![
                DiskColSchema {
                    name: "id".into(),
                    kind: DiskColKind::TextZstd,
                },
                DiskColSchema {
                    name: "wing".into(),
                    kind: DiskColKind::TextZstd,
                },
                DiskColSchema {
                    name: "room".into(),
                    kind: DiskColKind::TextZstd,
                },
                DiskColSchema {
                    name: "layer".into(),
                    kind: DiskColKind::TextZstd,
                },
            ],
            indexed_fields: vec!["wing".into()],
            cluster_by: None,
        },
    }])
    .unwrap();

    let mut rows = Vec::with_capacity(200);
    for i in 0..200 {
        let mut r = Row::new();
        r.insert("id".into(), Cell::from_text(format!("doc-{i}")));
        if i < 180 {
            r.insert("wing".into(), Cell::from_text(format!("wing-{}", i % 3)));
            r.insert("room".into(), Cell::from_text(format!("room-{}", i % 6)));
        }
        r.insert(
            "layer".into(),
            Cell::from_text(if i % 2 == 0 { "raw" } else { "wiki" }),
        );
        rows.push(r);
    }
    eng.commit_ops(vec![DiskWalOp::UpsertBatch {
        table: "docs".into(),
        rows,
    }])
    .unwrap();
    eng.checkpoint().unwrap();

    let seg_meta = &eng.manifest.segments[0];
    let kind_of = |name: &str| {
        seg_meta
            .columns
            .iter()
            .find(|c| c.name == name)
            .unwrap()
            .kind
    };
    assert_eq!(kind_of("id"), DiskColKind::TextZstd);
    assert_eq!(kind_of("wing"), DiskColKind::TextDict);
    assert_eq!(kind_of("room"), DiskColKind::TextDict);
    assert_eq!(kind_of("layer"), DiskColKind::TextDict);

    eng.commit_ops(vec![
        DiskWalOp::DeleteIds {
            table: "docs".into(),
            ids: vec!["doc-0".into(), "doc-199".into()],
        },
        DiskWalOp::UpsertBatch {
            table: "docs".into(),
            rows: vec![{
                let mut r = Row::new();
                r.insert("id".into(), Cell::from_text("doc-200"));
                r.insert("wing".into(), Cell::from_text("wing-0"));
                r.insert("room".into(), Cell::from_text("room-0"));
                r.insert("layer".into(), Cell::from_text("raw"));
                r
            }],
        },
    ])
    .unwrap();

    let (tot1, g1) = eng.group_count_str1("docs", "wing").unwrap();
    assert_eq!(tot1, 199);
    let m1: FxHashMap<Option<String>, u64> = g1.into_iter().collect();
    assert_eq!(m1.get(&Some("wing-0".into())), Some(&60));
    assert_eq!(m1.get(&Some("wing-1".into())), Some(&60));
    assert_eq!(m1.get(&Some("wing-2".into())), Some(&60));
    assert_eq!(m1.get(&None), Some(&19));

    let (tot2, g2) = eng.group_count_str2("docs", "wing", "room").unwrap();
    assert_eq!(tot2, 199);
    assert_eq!(g2.iter().map(|(_, c)| *c).sum::<u64>(), 199);

    let (tot3, g3) = eng
        .group_count_str3("docs", "wing", "room", "layer")
        .unwrap();
    assert_eq!(tot3, 199);
    assert_eq!(g3.iter().map(|(_, c)| *c).sum::<u64>(), 199);

    let mut w0_ids = Vec::new();
    eng.for_each_by_eq_str3(
        "docs",
        "wing",
        "wing-0",
        "id",
        "room",
        Some("layer"),
        |id, rm, ly| {
            w0_ids.push((
                id.unwrap_or_default().to_string(),
                rm.unwrap_or_default().to_string(),
                ly.unwrap_or_default().to_string(),
            ));
            Ok(true)
        },
    )
    .unwrap();
    assert_eq!(w0_ids.len(), 60);
    assert!(w0_ids.iter().any(|x| x.0 == "doc-200"));
    assert!(!w0_ids.iter().any(|x| x.0 == "doc-0"));

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn parallel_segment_writer_and_corrupt_mmap_index_rejection() {
    let tmp = std::env::temp_dir().join(format!("lin_disk_par_seg_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);

    let mut eng = DiskEngine::open(&tmp, false).unwrap();
    let mut rows = Vec::with_capacity(1_500);
    for i in 0..1_500usize {
        let mut r = Row::new();
        r.insert("id".into(), Cell::from_text(format!("item-{i}")));
        r.insert("cat".into(), Cell::from_text(format!("c-{}", i % 5)));
        r.insert("qty".into(), Cell::Int(i as i64));
        r.insert("price".into(), Cell::Float((i as f64) * 1.25));
        r.insert("active".into(), Cell::Bool(i % 2 == 0));
        rows.push(r);
    }
    eng.commit_ops(vec![
        DiskWalOp::EnsureTable {
            schema: DiskTableSchema {
                name: "items".into(),
                columns: vec![
                    DiskColSchema {
                        name: "id".into(),
                        kind: DiskColKind::TextZstd,
                    },
                    DiskColSchema {
                        name: "cat".into(),
                        kind: DiskColKind::TextZstd,
                    },
                    DiskColSchema {
                        name: "qty".into(),
                        kind: DiskColKind::Int64,
                    },
                    DiskColSchema {
                        name: "price".into(),
                        kind: DiskColKind::Float64,
                    },
                    DiskColSchema {
                        name: "active".into(),
                        kind: DiskColKind::Bool,
                    },
                ],
                indexed_fields: vec!["cat".into(), "active".into()],
                cluster_by: None,
            },
        },
        DiskWalOp::UpsertBatch {
            table: "items".into(),
            rows,
        },
    ])
    .unwrap();
    eng.checkpoint().unwrap();

    assert_eq!(eng.count_table("items"), 1_500);
    assert_eq!(
        eng.count_by_eq("items", "cat", &Cell::from_text("c-3"))
            .unwrap(),
        300
    );
    assert_eq!(
        eng.count_by_eq("items", "active", &Cell::Bool(true))
            .unwrap(),
        750
    );
    let r = eng.get_by_id("items", "item-1234").unwrap().unwrap();
    assert_eq!(r.get("qty"), Some(&Cell::Int(1234)));

    // Corrupt open-addressed hash table header (non-power-of-two mask + overflow) must fail cleanly
    let bad_ht = tmp.join("corrupt.ht");
    let mut ht_bytes = vec![0u8; 32];
    ht_bytes[0..4].copy_from_slice(b"LHT1");
    ht_bytes[4..8].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes()); // non-power-of-two mask+1
    fs::write(&bad_ht, &ht_bytes).unwrap();
    assert!(MmapHashIndex::open(&bad_ht).is_err());

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn clustered_compaction_orders_rows_and_tightens_zone_maps() {
    let tmp = std::env::temp_dir().join(format!("lin_disk_cluster_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    let mut eng = DiskEngine::open(&tmp, false).unwrap();
    eng.commit_ops(vec![DiskWalOp::EnsureTable {
        schema: DiskTableSchema {
            name: "ev".into(),
            columns: vec![
                DiskColSchema {
                    name: "id".into(),
                    kind: DiskColKind::TextZstd,
                },
                DiskColSchema {
                    name: "ts".into(),
                    kind: DiskColKind::Int64,
                },
                DiskColSchema {
                    name: "kind".into(),
                    kind: DiskColKind::TextDict,
                },
            ],
            indexed_fields: Vec::new(),
            cluster_by: None,
        },
    }])
    .unwrap();

    // 20 000 rows whose `ts` is scattered: every zone block spans almost the whole range.
    const N: usize = 20_000;
    for b in 0..2usize {
        let mut rows = Vec::with_capacity(N / 2);
        for i in 0..N / 2 {
            let gid = b * (N / 2) + i;
            let mut r = Row::new();
            r.insert("id".into(), Cell::from_text(format!("e-{gid}")));
            if gid % 1000 != 999 {
                r.insert("ts".into(), Cell::Int(((gid * 7919) % N) as i64));
            }
            r.insert("kind".into(), Cell::from_text(["a", "b", "c"][gid % 3]));
            rows.push(r);
        }
        eng.commit_ops(vec![DiskWalOp::UpsertBatch {
            table: "ev".into(),
            rows,
        }])
        .unwrap();
        eng.checkpoint().unwrap();
    }
    eng.commit_ops(vec![DiskWalOp::DeleteIds {
        table: "ev".into(),
        ids: (0..500).map(|i| format!("e-{i}")).collect(),
    }])
    .unwrap();

    let pred = crate::parse::parse_pred_src("ts >= 5000 and ts < 5200").unwrap();
    let ids_matching = |eng: &DiskEngine| -> Vec<String> {
        let mut ids: Vec<String> = eng
            .query_filter("ev", Some(&pred), None, Some(&["id"]), 0)
            .unwrap()
            .iter()
            .filter_map(|r| r.get("id").and_then(Cell::text).map(str::to_string))
            .collect();
        ids.sort();
        ids
    };
    let blocks_overlapping = |eng: &DiskEngine| -> usize {
        eng.segments["ev"]
            .iter()
            .map(|seg| {
                let fc = seg.fixed_col("ts").unwrap();
                (0..(seg.meta.row_count as usize).div_ceil(4096))
                    .filter(|&bi| {
                        !fc.zone_block_skips(bi, crate::ast::CmpOp::Ge, &Cell::Int(5000))
                            && !fc.zone_block_skips(bi, crate::ast::CmpOp::Lt, &Cell::Int(5200))
                    })
                    .count()
            })
            .sum()
    };

    let before_ids = ids_matching(&eng);
    let before_count = eng.count_table("ev");
    let before_blocks = blocks_overlapping(&eng);
    assert!(!before_ids.is_empty());
    assert!(before_blocks >= 4, "scattered data should touch most blocks");

    let rep = eng.compact_table_clustered("ev", "ts").unwrap();
    assert_eq!(rep.tables_compacted, 1);
    assert_eq!(rep.dead_rows_reclaimed, 500);

    assert_eq!(eng.count_table("ev"), before_count);
    assert_eq!(ids_matching(&eng), before_ids);
    assert!(eng.get_by_id("ev", "e-10").unwrap().is_none());
    assert!(eng.get_by_id("ev", "e-777").unwrap().is_some());
    let after_blocks = blocks_overlapping(&eng);
    assert!(
        after_blocks <= 2 && after_blocks < before_blocks,
        "clustered data should confine the range to at most two blocks, got {after_blocks} (was {before_blocks})"
    );

    // Rows come back ordered by `ts`, with missing values last.
    let mut prev: Option<i64> = None;
    let mut seen_null = false;
    eng.scan_projected("ev", Some(&["ts"]), |row| {
        match row.get("ts") {
            Some(Cell::Int(v)) => {
                assert!(!seen_null, "non-null after null");
                assert!(prev.is_none_or(|p| p <= *v), "not sorted: {prev:?} then {v}");
                prev = Some(*v);
            }
            _ => seen_null = true,
        }
        Ok(true)
    })
    .unwrap();
    assert!(seen_null);

    // A cluster key declared in the schema is applied by ordinary compaction and survives
    // a reopen. New unsorted rows first widen the range again...
    assert!(eng.set_cluster_key("ev", Some("nope")).is_err());
    eng.set_cluster_key("ev", Some("ts")).unwrap();
    let mut rows = Vec::new();
    for i in 0..9_000usize {
        let mut r = Row::new();
        r.insert("id".into(), Cell::from_text(format!("late-{i}")));
        r.insert("ts".into(), Cell::Int(((i * 6007) % N) as i64));
        r.insert("kind".into(), Cell::from_text("late"));
        rows.push(r);
    }
    eng.commit_ops(vec![DiskWalOp::UpsertBatch {
        table: "ev".into(),
        rows,
    }])
    .unwrap();
    eng.checkpoint().unwrap();
    let expected_ids = ids_matching(&eng);
    assert!(expected_ids.len() > before_ids.len());
    let total = eng.count_table("ev");
    // The delta segment written by that checkpoint is already in key order.
    {
        let newest = eng.segments["ev"].last().unwrap();
        let fc = newest.fixed_col("ts").unwrap();
        let mut prev = i64::MIN;
        for rid in 0..newest.meta.row_count {
            if !fc.is_null(rid) {
                let v = fc.read_i64(rid);
                assert!(prev <= v, "checkpointed delta segment is not sorted");
                prev = v;
            }
        }
        assert_eq!(newest.meta.row_count, 9_000);
    }
    drop(eng);

    let mut eng = DiskEngine::open(&tmp, false).unwrap();
    assert_eq!(
        eng.manifest.tables["ev"].cluster_by.as_deref(),
        Some("ts"),
        "cluster key must be persisted in the manifest"
    );
    eng.commit_ops(vec![DiskWalOp::DeleteIds {
        table: "ev".into(),
        ids: vec!["late-0".into()],
    }])
    .unwrap();
    let late0_in_range = expected_ids.iter().any(|id| id == "late-0");
    // ...and a plain vacuum restores the order without being told the key.
    let rep = eng.vacuum().unwrap();
    assert_eq!(rep.tables_compacted, 1);
    assert_eq!(eng.count_table("ev"), total - 1);
    assert_eq!(
        ids_matching(&eng).len(),
        expected_ids.len() - usize::from(late0_in_range)
    );
    assert!(
        blocks_overlapping(&eng) <= 2,
        "vacuum of a clustered table should keep the range in at most two blocks, got {}",
        blocks_overlapping(&eng)
    );
    let mut prev: Option<i64> = None;
    eng.scan_projected("ev", Some(&["ts"]), |row| {
        if let Some(Cell::Int(v)) = row.get("ts") {
            assert!(prev.is_none_or(|p| p <= *v), "not sorted after vacuum");
            prev = Some(*v);
        }
        Ok(true)
    })
    .unwrap();

    drop(eng);
    let eng = DiskEngine::open(&tmp, false).unwrap();
    assert_eq!(eng.count_table("ev"), total - 1);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn bulk_segment_from_record_batch_matches_row_path() {
    let base = std::env::temp_dir().join(format!("lin_disk_batch_ingest_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let (dir_rows, dir_batch) = (base.join("rows"), base.join("batch"));

    const N: usize = 6_000;
    let names: Arc<[String]> = ["id", "kind", "note", "qty", "price", "ok", "embedding"]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .into();
    let mut batch = crate::batch::RecordBatch::with_capacity(Arc::clone(&names), N);
    let mut rows = Vec::with_capacity(N);
    for i in 0..N {
        let mut v = [0.0f32; 8];
        v[i % 8] = 1.0;
        v[(i + 3) % 8] = (i % 13) as f32 * 0.01;
        let cells = [
            Cell::from_text(format!("b-{i}")),
            Cell::from_text(["red", "green", "blue"][i % 3]),
            // Absent every seventh row, unique otherwise (stays block-compressed text).
            if i % 7 == 0 {
                Cell::Null
            } else {
                Cell::from_text(format!("note number {i}"))
            },
            Cell::Int((i as i64 * 37) % 1000),
            if i % 11 == 0 {
                Cell::Null
            } else {
                Cell::Float(i as f64 * 0.5)
            },
            Cell::Bool(i % 2 == 0),
            Cell::Vec(Arc::from(v)),
        ];
        let mut r = Row::new();
        for (name, cell) in names.iter().zip(cells.iter()) {
            if !matches!(cell, Cell::Null) {
                r.insert(name.clone(), cell.clone());
            }
        }
        rows.push(r);
        for (col, cell) in batch.cols.iter_mut().zip(cells) {
            col.push(cell);
        }
    }

    let mut by_rows = DiskEngine::open(&dir_rows, false).unwrap();
    by_rows.write_bulk_segment("t", &rows).unwrap();
    by_rows.save_tombstones_and_manifest().unwrap();
    let mut by_batch = DiskEngine::open(&dir_batch, false).unwrap();
    by_batch.write_bulk_segment_batch("t", &batch).unwrap();
    by_batch.save_tombstones_and_manifest().unwrap();

    let dump = |eng: &DiskEngine| -> Vec<Row> {
        let mut out = Vec::new();
        eng.scan_projected("t", None, |r| {
            out.push(r.clone());
            Ok(true)
        })
        .unwrap();
        out
    };
    let kinds = |eng: &DiskEngine| -> Vec<(String, DiskColKind)> {
        let mut k: Vec<_> = eng.segments["t"][0]
            .meta
            .columns
            .iter()
            .map(|c| (c.name.clone(), c.kind))
            .collect();
        k.sort_by(|a, b| a.0.cmp(&b.0));
        k
    };
    assert_eq!(by_batch.count_table("t"), N);
    assert_eq!(kinds(&by_batch), kinds(&by_rows), "column encodings differ");
    assert!(
        kinds(&by_batch)
            .iter()
            .any(|(n, k)| n == "kind" && *k == DiskColKind::TextDict),
        "low-cardinality column should be dictionary-encoded"
    );
    assert_eq!(dump(&by_batch), dump(&by_rows));
    assert_eq!(dump(&by_batch), rows);
    assert_eq!(
        by_batch.get_by_id("t", "b-4242").unwrap(),
        Some(rows[4242].clone())
    );
    assert_eq!(
        by_batch.sum_f64_col("t", "qty").unwrap(),
        by_rows.sum_f64_col("t", "qty").unwrap()
    );

    // Not an upsert: an id that is already live, a repeated id, or no id column is refused
    // and leaves the table untouched.
    let one = |id: &str| {
        let mut b = crate::batch::RecordBatch::with_capacity(
            Arc::<[String]>::from(vec!["id".to_string(), "qty".to_string()]),
            2,
        );
        b.cols[0].push(Cell::from_text(id));
        b.cols[1].push(Cell::Int(1));
        b
    };
    assert!(by_batch.write_bulk_segment_batch("t", &one("b-17")).is_err());
    let mut dup = one("fresh-1");
    dup.cols[0].push(Cell::from_text("fresh-1"));
    dup.cols[1].push(Cell::Int(2));
    assert!(by_batch.write_bulk_segment_batch("t", &dup).is_err());
    let mut no_id = crate::batch::RecordBatch::with_capacity(
        Arc::<[String]>::from(vec!["qty".to_string()]),
        1,
    );
    no_id.cols[0].push(Cell::Int(1));
    assert!(by_batch.write_bulk_segment_batch("t", &no_id).is_err());
    assert_eq!(by_batch.count_table("t"), N);
    assert_eq!(by_batch.segments["t"].len(), 1);

    // A second batch with a new column appends another segment and extends the schema.
    let mut more = crate::batch::RecordBatch::with_capacity(
        Arc::<[String]>::from(vec!["id".to_string(), "qty".to_string(), "tag".to_string()]),
        1,
    );
    more.cols[0].push(Cell::from_text("fresh-2"));
    more.cols[1].push(Cell::Int(5));
    more.cols[2].push(Cell::from_text("late"));
    by_batch.write_bulk_segment_batch("t", &more).unwrap();
    by_batch.save_tombstones_and_manifest().unwrap();
    drop(by_batch);
    let reopened = DiskEngine::open(&dir_batch, false).unwrap();
    assert_eq!(reopened.count_table("t"), N + 1);
    let late = reopened.get_by_id("t", "fresh-2").unwrap().unwrap();
    assert_eq!(late.get("tag").and_then(Cell::text), Some("late"));
    assert_eq!(reopened.get_by_id("t", "b-9").unwrap(), Some(rows[9].clone()));

    let _ = fs::remove_dir_all(&base);
}

/// Table schemas travel inside MessagePack WAL frames and the JSON manifest. Data written
/// before `cluster_by` existed must still decode, or replay would drop those frames.
#[test]
fn table_schema_without_cluster_key_still_decodes() {
    #[derive(serde::Serialize)]
    struct OldSchema {
        name: String,
        columns: Vec<DiskColSchema>,
        indexed_fields: Vec<String>,
    }
    let old = OldSchema {
        name: "t".into(),
        columns: vec![DiskColSchema {
            name: "id".into(),
            kind: DiskColKind::TextZstd,
        }],
        indexed_fields: vec!["id".into()],
    };
    let from_msgpack: DiskTableSchema =
        rmp_serde::from_slice(&rmp_serde::to_vec(&old).unwrap()).unwrap();
    assert_eq!(from_msgpack.name, "t");
    assert_eq!(from_msgpack.indexed_fields, vec!["id".to_string()]);
    assert_eq!(from_msgpack.cluster_by, None);
    let from_json: DiskTableSchema =
        serde_json::from_slice(&serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(from_json.columns.len(), 1);
    assert_eq!(from_json.cluster_by, None);

    // The same holds for a whole WAL op carrying the old schema shape.
    #[derive(serde::Serialize)]
    enum OldOp {
        EnsureTable { schema: OldSchema },
    }
    let bytes = rmp_serde::to_vec(&OldOp::EnsureTable { schema: old }).unwrap();
    let op: DiskWalOp = rmp_serde::from_slice(&bytes).unwrap();
    assert!(matches!(op, DiskWalOp::EnsureTable { schema } if schema.cluster_by.is_none()));
}
