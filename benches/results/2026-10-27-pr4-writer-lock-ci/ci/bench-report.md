# bench 1793062696106467082-9625

Status: Complete

| Case | Metric | Median | Unit | Scope / statistic | Observations | Processes |
|---|---|---:|---|---|---:|---:|
| compare/point_get/lin [candidate] | wall | 161.7206 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/point_get/sqlite [candidate] | wall | 502.5286 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/point_get/duckdb [candidate] | wall | 249340.1250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/point_get/hashmap [candidate] | wall | 14.3535 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_eq/lin [candidate] | wall | 1058.7314 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_eq/sqlite [candidate] | wall | 121668.6875 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_eq/duckdb [candidate] | wall | 197110.6875 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_range/lin [candidate] | wall | 1761.1055 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_range/sqlite [candidate] | wall | 120358.8750 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_range/duckdb [candidate] | wall | 361778.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/text_substr/lin [candidate] | wall | 41328.1719 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_lex_selective/lin [candidate] | wall | 972.0679 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_lex_common/lin [candidate] | wall | 185328.5625 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_lex_miss/lin [candidate] | wall | 524.7422 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_hybrid_common/lin [candidate] | wall | 6221301.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/text_substr/sqlite [candidate] | wall | 422701.8750 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/text_substr/duckdb [candidate] | wall | 264407.8750 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/materialize/lin [candidate] | wall | 784168.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/materialize/sqlite [candidate] | wall | 1201914.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/materialize/duckdb [candidate] | wall | 1131067.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/lin [candidate] | wall | 361905.1250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/lin_cursor [candidate] | wall | 1874129.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/lin_cursor_projected [candidate] | wall | 607687.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_phase/lin_cursor_open [candidate] | wall | 14901.0859 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_phase/lin_cursor_scan_project [candidate] | wall | 2385526.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_phase/lin_cursor_scan_projected_row [candidate] | wall | 1074712.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/sqlite [candidate] | wall | 3172188.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/duckdb [candidate] | wall | 2306111.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/lin [candidate] | wall | 193052.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/lin_cursor [candidate] | wall | 960915.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/sqlite [candidate] | wall | 1697988.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/duckdb [candidate] | wall | 1509829.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum/lin [candidate] | wall | 8342.8281 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum/sqlite [candidate] | wall | 301094.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum/duckdb [candidate] | wall | 117068.3125 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum_by/lin [candidate] | wall | 353237.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum_by/sqlite [candidate] | wall | 1890298.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum_by/duckdb [candidate] | wall | 632129.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_count_by/lin [candidate] | wall | 110598.8438 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_count_by/sqlite [candidate] | wall | 372601.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_count_by/duckdb [candidate] | wall | 501221.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_1k/lin [candidate] | wall | 1320427.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_1k/sqlite [candidate] | wall | 1178063.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_1k/duckdb [candidate] | wall | 255930029.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_10k/lin [candidate] | wall | 20243780.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_10k/sqlite [candidate] | wall | 15234793.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_10k/duckdb [candidate] | wall | 2575578758.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_native_1k/duckdb_appender [candidate] | wall | 2329871.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_native_10k/duckdb_appender [candidate] | wall | 19004776.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_full [candidate] | wall | 20548696.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_no_embed [candidate] | wall | 9380594.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_no_embed_no_scalar_index [candidate] | wall | 7426885.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts [candidate] | wall | 5333095.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_1k/lin [candidate] | wall | 211717.0625 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_1k/sqlite [candidate] | wall | 593343.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_1k/duckdb [candidate] | wall | 1302411.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_10k/lin [candidate] | wall | 2159285.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_10k/sqlite [candidate] | wall | 7234346.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_10k/duckdb [candidate] | wall | 9677830.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/group_commit_16/lin_sequential_full [candidate] | wall | 2637883.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/group_commit_16/lin_grouped_full [candidate] | wall | 290950.3750 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reopen_phase_5k/rebuild_row_maps [candidate] | wall | 1118073.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reopen_phase_5k/rebuild_scalar_indexes [candidate] | wall | 858588.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reopen_phase_5k/rebuild_fts [candidate] | wall | 1186861.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/plan_dnf_6/lin [candidate] | wall | 144767.9375 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/plan_dnf_10/lin [candidate] | wall | 682315.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/plan_dnf_14/lin [candidate] | wall | 706912.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/graph_depth_1/lin [candidate] | wall | 170601.9375 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/graph_depth_2/lin [candidate] | wall | 206467.8750 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/graph_depth_3/lin [candidate] | wall | 239966.6250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reader_snapshot/lin [candidate] | wall | 11494592.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |

# Useful work throughput

| Case | Median batch throughput | Unit |
|---|---:|---|
| compare/join_inner/lin [candidate] | 27631663.6115 | rows/s |
| compare/join_inner/lin_cursor [candidate] | 5335815.5630 | rows/s |
| compare/join_inner/lin_cursor_projected [candidate] | 16455838.9556 | rows/s |
| compare/join_phase/lin_cursor_scan_project [candidate] | 4191955.1471 | rows/s |
| compare/join_phase/lin_cursor_scan_projected_row [candidate] | 9304903.7623 | rows/s |
| compare/join_inner/sqlite [candidate] | 3152397.7858 | rows/s |
| compare/join_inner/duckdb [candidate] | 4336396.3708 | rows/s |
| compare/join_filter/lin [candidate] | 25899775.2854 | rows/s |
| compare/join_filter/lin_cursor [candidate] | 5203391.6644 | rows/s |
| compare/join_filter/sqlite [candidate] | 2944685.1444 | rows/s |
| compare/join_filter/duckdb [candidate] | 3311633.3623 | rows/s |
| compare/agg_sum/lin [candidate] | 1198634340.0929 | rows/s |
| compare/agg_sum/sqlite [candidate] | 33212164.2896 | rows/s |
| compare/agg_sum/duckdb [candidate] | 85423921.3722 | rows/s |
| compare/agg_sum_by/lin [candidate] | 28309605.1663 | rows/s |
| compare/agg_sum_by/sqlite [candidate] | 5290171.2478 | rows/s |
| compare/agg_sum_by/duckdb [candidate] | 15823041.8498 | rows/s |
| compare/agg_count_by/lin [candidate] | 90416864.8412 | rows/s |
| compare/agg_count_by/sqlite [candidate] | 26838344.4928 | rows/s |
| compare/agg_count_by/duckdb [candidate] | 19955830.9386 | rows/s |
| compare/insert_bulk_1k/lin [candidate] | 757338.7487 | rows/s |
| compare/insert_bulk_1k/sqlite [candidate] | 848851.1366 | rows/s |
| compare/insert_bulk_1k/duckdb [candidate] | 3907.3289 | rows/s |
| compare/insert_bulk_10k/lin [candidate] | 494009.3004 | rows/s |
| compare/insert_bulk_10k/sqlite [candidate] | 656392.4276 | rows/s |
| compare/insert_bulk_10k/duckdb [candidate] | 3882.6225 | rows/s |
| compare/insert_native_1k/duckdb_appender [candidate] | 429251.7437 | rows/s |
| compare/insert_native_10k/duckdb_appender [candidate] | 526185.5359 | rows/s |
| compare/insert_phase_10k/lin_full [candidate] | 486652.7073 | rows/s |
| compare/insert_phase_10k/lin_no_embed [candidate] | 1066031.1278 | rows/s |
| compare/insert_phase_10k/lin_no_embed_no_scalar_index [candidate] | 1346478.5161 | rows/s |
| compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts [candidate] | 1875123.4574 | rows/s |
| compare/append_log_1k/lin [candidate] | 4723299.4467 | rows/s |
| compare/append_log_1k/sqlite [candidate] | 1685369.0113 | rows/s |
| compare/append_log_1k/duckdb [candidate] | 767806.8613 | rows/s |
| compare/append_log_10k/lin [candidate] | 4631234.0245 | rows/s |
| compare/append_log_10k/sqlite [candidate] | 1382303.6991 | rows/s |
| compare/append_log_10k/duckdb [candidate] | 1033349.4670 | rows/s |
| compare/group_commit_16/lin_sequential_full [candidate] | 6065.4717 | commits/s |
| compare/group_commit_16/lin_grouped_full [candidate] | 55005.7703 | statements/s |
| compare/reopen_phase_5k/rebuild_row_maps [candidate] | 4471980.9052 | rows/s |
| compare/reopen_phase_5k/rebuild_scalar_indexes [candidate] | 5823512.5339 | rows/s |
| compare/reopen_phase_5k/rebuild_fts [candidate] | 4212798.2187 | rows/s |

Derived from positive wall batches and declared units per operation; zero/unavailable durations omitted.

Medians above summarize observations, not pooled operation-latency percentiles. Comparisons aggregate within each process first.

- Raw batches are not independent process replications. Batch averages are not individual-operation latency. CPU frequency and background load are uncontrolled.

- compare/agg_count_by/duckdb: some samples below half the requested duration; iteration cap or workload drift may limit precision

