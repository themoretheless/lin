# bench 1792996231061339633-9899

Status: Complete

| Case | Metric | Median | Unit | Scope / statistic | Observations | Processes |
|---|---|---:|---|---|---:|---:|
| compare/point_get/lin [candidate] | wall | 195.4196 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/point_get/sqlite [candidate] | wall | 706.4309 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/point_get/duckdb [candidate] | wall | 321967.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/point_get/hashmap [candidate] | wall | 18.0634 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_eq/lin [candidate] | wall | 1335.2192 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_eq/sqlite [candidate] | wall | 152030.8750 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_eq/duckdb [candidate] | wall | 311239.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_range/lin [candidate] | wall | 2138.4092 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_range/sqlite [candidate] | wall | 151302.0625 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_range/duckdb [candidate] | wall | 489216.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/text_substr/lin [candidate] | wall | 47420.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_lex_selective/lin [candidate] | wall | 1360.6333 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_lex_common/lin [candidate] | wall | 264045.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_lex_miss/lin [candidate] | wall | 690.2764 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_hybrid_common/lin [candidate] | wall | 7537623.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/text_substr/sqlite [candidate] | wall | 548066.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/text_substr/duckdb [candidate] | wall | 385222.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/materialize/lin [candidate] | wall | 1031658.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/materialize/sqlite [candidate] | wall | 1562228.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/materialize/duckdb [candidate] | wall | 1310348.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/lin [candidate] | wall | 526343.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/lin_cursor [candidate] | wall | 1993714.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/lin_cursor_projected [candidate] | wall | 822391.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_phase/lin_cursor_open [candidate] | wall | 19858.5781 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_phase/lin_cursor_scan_project [candidate] | wall | 2541157.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_phase/lin_cursor_scan_projected_row [candidate] | wall | 1353058.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/sqlite [candidate] | wall | 4604087.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/duckdb [candidate] | wall | 2726478.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/lin [candidate] | wall | 276471.6250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/lin_cursor [candidate] | wall | 1003179.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/sqlite [candidate] | wall | 2479738.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/duckdb [candidate] | wall | 1917753.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum/lin [candidate] | wall | 9537.0391 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum/sqlite [candidate] | wall | 372924.3750 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum/duckdb [candidate] | wall | 182736.9375 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum_by/lin [candidate] | wall | 432345.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum_by/sqlite [candidate] | wall | 2374060.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum_by/duckdb [candidate] | wall | 830642.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_count_by/lin [candidate] | wall | 166740.9375 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_count_by/sqlite [candidate] | wall | 483618.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_count_by/duckdb [candidate] | wall | 691076.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_1k/lin [candidate] | wall | 3204762.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_1k/sqlite [candidate] | wall | 1625572.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_1k/duckdb [candidate] | wall | 375231504.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_10k/lin [candidate] | wall | 34581857.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_10k/sqlite [candidate] | wall | 20412158.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_10k/duckdb [candidate] | wall | 3761045905.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_native_1k/duckdb_appender [candidate] | wall | 3291840.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_native_10k/duckdb_appender [candidate] | wall | 23946307.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_full [candidate] | wall | 35957685.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_no_embed [candidate] | wall | 15302962.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_no_embed_no_scalar_index [candidate] | wall | 10236053.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts [candidate] | wall | 7098576.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_1k/lin [candidate] | wall | 292131.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_1k/sqlite [candidate] | wall | 852471.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_1k/duckdb [candidate] | wall | 1780017.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_10k/lin [candidate] | wall | 3575674.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_10k/sqlite [candidate] | wall | 9927317.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_10k/duckdb [candidate] | wall | 12925303.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/group_commit_16/lin_sequential_full [candidate] | wall | 3729326.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/group_commit_16/lin_grouped_full [candidate] | wall | 379702.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reopen_phase_5k/rebuild_row_maps [candidate] | wall | 1571954.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reopen_phase_5k/rebuild_scalar_indexes [candidate] | wall | 1061968.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reopen_phase_5k/rebuild_fts [candidate] | wall | 1686408.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/plan_dnf_6/lin [candidate] | wall | 186650.8750 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/plan_dnf_10/lin [candidate] | wall | 871193.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/plan_dnf_14/lin [candidate] | wall | 904641.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/graph_depth_1/lin [candidate] | wall | 252669.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/graph_depth_2/lin [candidate] | wall | 316137.6250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/graph_depth_3/lin [candidate] | wall | 370816.1250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reader_snapshot/lin [candidate] | wall | 15788603.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |

# Useful work throughput

| Case | Median batch throughput | Unit |
|---|---:|---|
| compare/join_inner/lin [candidate] | 18999055.6018 | rows/s |
| compare/join_inner/lin_cursor [candidate] | 5015764.2266 | rows/s |
| compare/join_inner/lin_cursor_projected [candidate] | 12159744.6035 | rows/s |
| compare/join_phase/lin_cursor_scan_project [candidate] | 3936080.9160 | rows/s |
| compare/join_phase/lin_cursor_scan_projected_row [candidate] | 7390685.7898 | rows/s |
| compare/join_inner/sqlite [candidate] | 2171983.2973 | rows/s |
| compare/join_inner/duckdb [candidate] | 3667735.4916 | rows/s |
| compare/join_filter/lin [candidate] | 18085047.1392 | rows/s |
| compare/join_filter/lin_cursor [candidate] | 4984230.0068 | rows/s |
| compare/join_filter/sqlite [candidate] | 2016345.1416 | rows/s |
| compare/join_filter/duckdb [candidate] | 2608705.4910 | rows/s |
| compare/agg_sum/lin [candidate] | 1048543832.4038 | rows/s |
| compare/agg_sum/sqlite [candidate] | 26815223.9464 | rows/s |
| compare/agg_sum/duckdb [candidate] | 54775743.9022 | rows/s |
| compare/agg_sum_by/lin [candidate] | 23129700.2702 | rows/s |
| compare/agg_sum_by/sqlite [candidate] | 4212196.7665 | rows/s |
| compare/agg_sum_by/duckdb [candidate] | 12042147.1664 | rows/s |
| compare/agg_count_by/lin [candidate] | 59974284.9372 | rows/s |
| compare/agg_count_by/sqlite [candidate] | 20677566.2728 | rows/s |
| compare/agg_count_by/duckdb [candidate] | 14470178.2165 | rows/s |
| compare/insert_bulk_1k/lin [candidate] | 312036.1951 | rows/s |
| compare/insert_bulk_1k/sqlite [candidate] | 615168.0852 | rows/s |
| compare/insert_bulk_1k/duckdb [candidate] | 2665.0308 | rows/s |
| compare/insert_bulk_10k/lin [candidate] | 289169.2361 | rows/s |
| compare/insert_bulk_10k/sqlite [candidate] | 489904.1312 | rows/s |
| compare/insert_bulk_10k/duckdb [candidate] | 2658.8370 | rows/s |
| compare/insert_native_1k/duckdb_appender [candidate] | 303860.2037 | rows/s |
| compare/insert_native_10k/duckdb_appender [candidate] | 417612.6261 | rows/s |
| compare/insert_phase_10k/lin_full [candidate] | 278109.6496 | rows/s |
| compare/insert_phase_10k/lin_no_embed [candidate] | 653468.2682 | rows/s |
| compare/insert_phase_10k/lin_no_embed_no_scalar_index [candidate] | 977010.5493 | rows/s |
| compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts [candidate] | 1410877.1833 | rows/s |
| compare/append_log_1k/lin [candidate] | 3423122.4419 | rows/s |
| compare/append_log_1k/sqlite [candidate] | 1173060.3609 | rows/s |
| compare/append_log_1k/duckdb [candidate] | 561792.9236 | rows/s |
| compare/append_log_10k/lin [candidate] | 2796679.7301 | rows/s |
| compare/append_log_10k/sqlite [candidate] | 1007321.6101 | rows/s |
| compare/append_log_10k/duckdb [candidate] | 773681.4169 | rows/s |
| compare/group_commit_16/lin_sequential_full [candidate] | 4290.3742 | commits/s |
| compare/group_commit_16/lin_grouped_full [candidate] | 42143.4007 | statements/s |
| compare/reopen_phase_5k/rebuild_row_maps [candidate] | 3180769.3783 | rows/s |
| compare/reopen_phase_5k/rebuild_scalar_indexes [candidate] | 4708468.3577 | rows/s |
| compare/reopen_phase_5k/rebuild_fts [candidate] | 2964883.6199 | rows/s |

Derived from positive wall batches and declared units per operation; zero/unavailable durations omitted.

Medians above summarize observations, not pooled operation-latency percentiles. Comparisons aggregate within each process first.

- Raw batches are not independent process replications. Batch averages are not individual-operation latency. CPU frequency and background load are uncontrolled.

