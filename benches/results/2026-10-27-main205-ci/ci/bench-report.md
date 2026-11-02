# bench 1793044826899690298-2832

Status: Complete

| Case | Metric | Median | Unit | Scope / statistic | Observations | Processes |
|---|---|---:|---|---|---:|---:|
| compare/point_get/lin [candidate] | wall | 199.4504 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/point_get/sqlite [candidate] | wall | 728.6934 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/point_get/duckdb [candidate] | wall | 315941.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/point_get/hashmap [candidate] | wall | 18.5766 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_eq/lin [candidate] | wall | 1364.7437 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_eq/sqlite [candidate] | wall | 154223.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_eq/duckdb [candidate] | wall | 289794.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_range/lin [candidate] | wall | 2056.6621 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_range/sqlite [candidate] | wall | 152321.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_range/duckdb [candidate] | wall | 467886.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/text_substr/lin [candidate] | wall | 47710.2812 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_lex_selective/lin [candidate] | wall | 1342.9165 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_lex_common/lin [candidate] | wall | 262293.6250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_lex_miss/lin [candidate] | wall | 691.2290 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_hybrid_common/lin [candidate] | wall | 7335128.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/text_substr/sqlite [candidate] | wall | 545586.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/text_substr/duckdb [candidate] | wall | 369303.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/materialize/lin [candidate] | wall | 981327.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/materialize/sqlite [candidate] | wall | 1571086.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/materialize/duckdb [candidate] | wall | 1210169.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/lin [candidate] | wall | 494646.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/lin_cursor [candidate] | wall | 1954165.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/lin_cursor_projected [candidate] | wall | 822752.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_phase/lin_cursor_open [candidate] | wall | 19438.3984 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_phase/lin_cursor_scan_project [candidate] | wall | 2449287.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_phase/lin_cursor_scan_projected_row [candidate] | wall | 1371405.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/sqlite [candidate] | wall | 4339438.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/duckdb [candidate] | wall | 2838118.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/lin [candidate] | wall | 269401.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/lin_cursor [candidate] | wall | 1003147.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/sqlite [candidate] | wall | 2388732.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/duckdb [candidate] | wall | 1898254.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum/lin [candidate] | wall | 9527.4648 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum/sqlite [candidate] | wall | 375733.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum/duckdb [candidate] | wall | 186345.0625 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum_by/lin [candidate] | wall | 431231.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum_by/sqlite [candidate] | wall | 2343623.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum_by/duckdb [candidate] | wall | 843260.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_count_by/lin [candidate] | wall | 166556.9375 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_count_by/sqlite [candidate] | wall | 481005.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_count_by/duckdb [candidate] | wall | 698275.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_1k/lin [candidate] | wall | 1748089.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_1k/sqlite [candidate] | wall | 1580497.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_1k/duckdb [candidate] | wall | 371312388.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_10k/lin [candidate] | wall | 33217287.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_10k/sqlite [candidate] | wall | 19876115.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_10k/duckdb [candidate] | wall | 3705159366.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_native_1k/duckdb_appender [candidate] | wall | 3157998.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_native_10k/duckdb_appender [candidate] | wall | 26228098.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_full [candidate] | wall | 32861880.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_no_embed [candidate] | wall | 12143564.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_no_embed_no_scalar_index [candidate] | wall | 8390332.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts [candidate] | wall | 5825107.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_1k/lin [candidate] | wall | 268651.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_1k/sqlite [candidate] | wall | 855550.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_1k/duckdb [candidate] | wall | 1694769.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_10k/lin [candidate] | wall | 2881127.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_10k/sqlite [candidate] | wall | 9897458.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_10k/duckdb [candidate] | wall | 12136486.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/group_commit_16/lin_sequential_full [candidate] | wall | 3111652.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/group_commit_16/lin_grouped_full [candidate] | wall | 320896.1250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reopen_phase_5k/rebuild_row_maps [candidate] | wall | 1561787.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reopen_phase_5k/rebuild_scalar_indexes [candidate] | wall | 1056491.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reopen_phase_5k/rebuild_fts [candidate] | wall | 1664062.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/plan_dnf_6/lin [candidate] | wall | 187025.6250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/plan_dnf_10/lin [candidate] | wall | 877248.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/plan_dnf_14/lin [candidate] | wall | 912183.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/graph_depth_1/lin [candidate] | wall | 253185.1250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/graph_depth_2/lin [candidate] | wall | 311763.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/graph_depth_3/lin [candidate] | wall | 366686.1250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reader_snapshot/lin [candidate] | wall | 13596118.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |

# Useful work throughput

| Case | Median batch throughput | Unit |
|---|---:|---|
| compare/join_inner/lin [candidate] | 20216482.9827 | rows/s |
| compare/join_inner/lin_cursor [candidate] | 5117274.4215 | rows/s |
| compare/join_inner/lin_cursor_projected [candidate] | 12154368.2359 | rows/s |
| compare/join_phase/lin_cursor_scan_project [candidate] | 4082820.0947 | rows/s |
| compare/join_phase/lin_cursor_scan_projected_row [candidate] | 7291802.5881 | rows/s |
| compare/join_inner/sqlite [candidate] | 2304446.3741 | rows/s |
| compare/join_inner/duckdb [candidate] | 3523903.3026 | rows/s |
| compare/join_filter/lin [candidate] | 18559659.1537 | rows/s |
| compare/join_filter/lin_cursor [candidate] | 4984335.6684 | rows/s |
| compare/join_filter/sqlite [candidate] | 2093161.0652 | rows/s |
| compare/join_filter/duckdb [candidate] | 2634001.0596 | rows/s |
| compare/agg_sum/lin [candidate] | 1049597209.1580 | rows/s |
| compare/agg_sum/sqlite [candidate] | 26615209.8465 | rows/s |
| compare/agg_sum/duckdb [candidate] | 53663901.4718 | rows/s |
| compare/agg_sum_by/lin [candidate] | 23189387.1468 | rows/s |
| compare/agg_sum_by/sqlite [candidate] | 4266898.1201 | rows/s |
| compare/agg_sum_by/duckdb [candidate] | 11859773.0236 | rows/s |
| compare/agg_count_by/lin [candidate] | 60039533.2928 | rows/s |
| compare/agg_count_by/sqlite [candidate] | 20789783.0690 | rows/s |
| compare/agg_count_by/duckdb [candidate] | 14321026.3560 | rows/s |
| compare/insert_bulk_1k/lin [candidate] | 572054.0372 | rows/s |
| compare/insert_bulk_1k/sqlite [candidate] | 632712.1735 | rows/s |
| compare/insert_bulk_1k/duckdb [candidate] | 2693.1513 | rows/s |
| compare/insert_bulk_10k/lin [candidate] | 301048.5069 | rows/s |
| compare/insert_bulk_10k/sqlite [candidate] | 503116.5790 | rows/s |
| compare/insert_bulk_10k/duckdb [candidate] | 2698.9397 | rows/s |
| compare/insert_native_1k/duckdb_appender [candidate] | 317676.6325 | rows/s |
| compare/insert_native_10k/duckdb_appender [candidate] | 381270.6576 | rows/s |
| compare/insert_phase_10k/lin_full [candidate] | 304305.1194 | rows/s |
| compare/insert_phase_10k/lin_no_embed [candidate] | 823486.5234 | rows/s |
| compare/insert_phase_10k/lin_no_embed_no_scalar_index [candidate] | 1191864.3536 | rows/s |
| compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts [candidate] | 1717293.9434 | rows/s |
| compare/append_log_1k/lin [candidate] | 3724592.9991 | rows/s |
| compare/append_log_1k/sqlite [candidate] | 1168838.8311 | rows/s |
| compare/append_log_1k/duckdb [candidate] | 590050.9844 | rows/s |
| compare/append_log_10k/lin [candidate] | 3470868.3269 | rows/s |
| compare/append_log_10k/sqlite [candidate] | 1010365.8837 | rows/s |
| compare/append_log_10k/duckdb [candidate] | 823963.6354 | rows/s |
| compare/group_commit_16/lin_sequential_full [candidate] | 5142.0602 | commits/s |
| compare/group_commit_16/lin_grouped_full [candidate] | 49957.3727 | statements/s |
| compare/reopen_phase_5k/rebuild_row_maps [candidate] | 3201463.7772 | rows/s |
| compare/reopen_phase_5k/rebuild_scalar_indexes [candidate] | 4732696.7393 | rows/s |
| compare/reopen_phase_5k/rebuild_fts [candidate] | 3004695.4243 | rows/s |

Derived from positive wall batches and declared units per operation; zero/unavailable durations omitted.

Medians above summarize observations, not pooled operation-latency percentiles. Comparisons aggregate within each process first.

- Raw batches are not independent process replications. Batch averages are not individual-operation latency. CPU frequency and background load are uncontrolled.

- compare/text_substr/duckdb: some samples below half the requested duration; iteration cap or workload drift may limit precision

