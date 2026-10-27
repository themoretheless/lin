# bench 1793063422629944777-9936

Status: Complete

| Case | Metric | Median | Unit | Scope / statistic | Observations | Processes |
|---|---|---:|---|---|---:|---:|
| compare/point_get/lin [candidate] | wall | 198.8222 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/point_get/sqlite [candidate] | wall | 693.0420 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/point_get/duckdb [candidate] | wall | 326340.3750 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/point_get/hashmap [candidate] | wall | 18.7610 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_eq/lin [candidate] | wall | 1360.4253 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_eq/sqlite [candidate] | wall | 153486.4375 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_eq/duckdb [candidate] | wall | 297383.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_range/lin [candidate] | wall | 2088.4756 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_range/sqlite [candidate] | wall | 151250.4375 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_range/duckdb [candidate] | wall | 482097.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/text_substr/lin [candidate] | wall | 47424.4688 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_lex_selective/lin [candidate] | wall | 1332.3901 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_lex_common/lin [candidate] | wall | 254463.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_lex_miss/lin [candidate] | wall | 700.5217 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_hybrid_common/lin [candidate] | wall | 7690866.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/text_substr/sqlite [candidate] | wall | 544113.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/text_substr/duckdb [candidate] | wall | 371410.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/materialize/lin [candidate] | wall | 1017107.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/materialize/sqlite [candidate] | wall | 1508697.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/materialize/duckdb [candidate] | wall | 1267926.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/lin [candidate] | wall | 503203.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/lin_cursor [candidate] | wall | 1965937.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/lin_cursor_projected [candidate] | wall | 832096.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_phase/lin_cursor_open [candidate] | wall | 19785.5234 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_phase/lin_cursor_scan_project [candidate] | wall | 2517394.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_phase/lin_cursor_scan_projected_row [candidate] | wall | 1423923.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/sqlite [candidate] | wall | 4333796.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/duckdb [candidate] | wall | 3009489.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/lin [candidate] | wall | 277519.1250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/lin_cursor [candidate] | wall | 984406.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/sqlite [candidate] | wall | 2440404.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/duckdb [candidate] | wall | 1974924.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum/lin [candidate] | wall | 9593.4102 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum/sqlite [candidate] | wall | 377431.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum/duckdb [candidate] | wall | 203537.5625 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum_by/lin [candidate] | wall | 444401.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum_by/sqlite [candidate] | wall | 2340558.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum_by/duckdb [candidate] | wall | 1136004.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_count_by/lin [candidate] | wall | 163910.9375 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_count_by/sqlite [candidate] | wall | 480225.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_count_by/duckdb [candidate] | wall | 939271.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_1k/lin [candidate] | wall | 1991355.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_1k/sqlite [candidate] | wall | 1609425.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_1k/duckdb [candidate] | wall | 383507600.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_10k/lin [candidate] | wall | 26579291.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_10k/sqlite [candidate] | wall | 20100217.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_10k/duckdb [candidate] | wall | 3858333890.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_native_1k/duckdb_appender [candidate] | wall | 3691071.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_native_10k/duckdb_appender [candidate] | wall | 25871449.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_full [candidate] | wall | 25871965.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_no_embed [candidate] | wall | 13152014.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_no_embed_no_scalar_index [candidate] | wall | 10536923.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts [candidate] | wall | 7590117.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_1k/lin [candidate] | wall | 309025.6250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_1k/sqlite [candidate] | wall | 846934.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_1k/duckdb [candidate] | wall | 1845667.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_10k/lin [candidate] | wall | 3682232.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_10k/sqlite [candidate] | wall | 9833398.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_10k/duckdb [candidate] | wall | 12473321.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/group_commit_16/lin_sequential_full [candidate] | wall | 3889805.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/group_commit_16/lin_grouped_full [candidate] | wall | 418192.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reopen_phase_5k/rebuild_row_maps [candidate] | wall | 1585150.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reopen_phase_5k/rebuild_scalar_indexes [candidate] | wall | 1125685.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reopen_phase_5k/rebuild_fts [candidate] | wall | 1776447.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/plan_dnf_6/lin [candidate] | wall | 186945.9375 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/plan_dnf_10/lin [candidate] | wall | 874493.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/plan_dnf_14/lin [candidate] | wall | 906195.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/graph_depth_1/lin [candidate] | wall | 247114.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/graph_depth_2/lin [candidate] | wall | 303909.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/graph_depth_3/lin [candidate] | wall | 363227.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reader_snapshot/lin [candidate] | wall | 18534198.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |

# Useful work throughput

| Case | Median batch throughput | Unit |
|---|---:|---|
| compare/join_inner/lin [candidate] | 19872693.7358 | rows/s |
| compare/join_inner/lin_cursor [candidate] | 5086636.5150 | rows/s |
| compare/join_inner/lin_cursor_projected [candidate] | 12017892.3277 | rows/s |
| compare/join_phase/lin_cursor_scan_project [candidate] | 3972364.4500 | rows/s |
| compare/join_phase/lin_cursor_scan_projected_row [candidate] | 7022926.3084 | rows/s |
| compare/join_inner/sqlite [candidate] | 2307447.8092 | rows/s |
| compare/join_inner/duckdb [candidate] | 3323322.5655 | rows/s |
| compare/join_filter/lin [candidate] | 18016804.2661 | rows/s |
| compare/join_filter/lin_cursor [candidate] | 5080515.4061 | rows/s |
| compare/join_filter/sqlite [candidate] | 2048841.5353 | rows/s |
| compare/join_filter/duckdb [candidate] | 2531976.9744 | rows/s |
| compare/agg_sum/lin [candidate] | 1042382348.2451 | rows/s |
| compare/agg_sum/sqlite [candidate] | 26495883.7453 | rows/s |
| compare/agg_sum/duckdb [candidate] | 49135202.2875 | rows/s |
| compare/agg_sum_by/lin [candidate] | 22502200.9687 | rows/s |
| compare/agg_sum_by/sqlite [candidate] | 4272491.1145 | rows/s |
| compare/agg_sum_by/duckdb [candidate] | 8802847.3931 | rows/s |
| compare/agg_count_by/lin [candidate] | 61008780.3405 | rows/s |
| compare/agg_count_by/sqlite [candidate] | 20823539.8893 | rows/s |
| compare/agg_count_by/duckdb [candidate] | 10647176.0552 | rows/s |
| compare/insert_bulk_1k/lin [candidate] | 502176.8253 | rows/s |
| compare/insert_bulk_1k/sqlite [candidate] | 621340.1492 | rows/s |
| compare/insert_bulk_1k/duckdb [candidate] | 2607.5138 | rows/s |
| compare/insert_bulk_10k/lin [candidate] | 376233.6016 | rows/s |
| compare/insert_bulk_10k/sqlite [candidate] | 497507.2913 | rows/s |
| compare/insert_bulk_10k/duckdb [candidate] | 2591.8575 | rows/s |
| compare/insert_native_1k/duckdb_appender [candidate] | 271089.2855 | rows/s |
| compare/insert_native_10k/duckdb_appender [candidate] | 387081.9715 | rows/s |
| compare/insert_phase_10k/lin_full [candidate] | 386519.2089 | rows/s |
| compare/insert_phase_10k/lin_no_embed [candidate] | 760342.1481 | rows/s |
| compare/insert_phase_10k/lin_no_embed_no_scalar_index [candidate] | 949043.6242 | rows/s |
| compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts [candidate] | 1317512.7936 | rows/s |
| compare/append_log_1k/lin [candidate] | 3236234.8361 | rows/s |
| compare/append_log_1k/sqlite [candidate] | 1180730.0310 | rows/s |
| compare/append_log_1k/duckdb [candidate] | 541832.5625 | rows/s |
| compare/append_log_10k/lin [candidate] | 2715790.6254 | rows/s |
| compare/append_log_10k/sqlite [candidate] | 1016942.9537 | rows/s |
| compare/append_log_10k/duckdb [candidate] | 801867.4104 | rows/s |
| compare/group_commit_16/lin_sequential_full [candidate] | 4115.0394 | commits/s |
| compare/group_commit_16/lin_grouped_full [candidate] | 38261.8359 | statements/s |
| compare/reopen_phase_5k/rebuild_row_maps [candidate] | 3154277.0231 | rows/s |
| compare/reopen_phase_5k/rebuild_scalar_indexes [candidate] | 4441906.9549 | rows/s |
| compare/reopen_phase_5k/rebuild_fts [candidate] | 2814633.3478 | rows/s |

Derived from positive wall batches and declared units per operation; zero/unavailable durations omitted.

Medians above summarize observations, not pooled operation-latency percentiles. Comparisons aggregate within each process first.

- Raw batches are not independent process replications. Batch averages are not individual-operation latency. CPU frequency and background load are uncontrolled.

- compare/fts_lex_common/lin: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/text_substr/duckdb: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/agg_sum_by/lin: some samples below half the requested duration; iteration cap or workload drift may limit precision

