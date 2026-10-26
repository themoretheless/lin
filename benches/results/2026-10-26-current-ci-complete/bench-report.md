# bench 1793041248908423859-9624

Status: Complete

| Case | Metric | Median | Unit | Scope / statistic | Observations | Processes |
|---|---|---:|---|---|---:|---:|
| compare/point_get/lin [candidate] | wall | 155.8232 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/point_get/sqlite [candidate] | wall | 374.5889 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/point_get/duckdb [candidate] | wall | 205692.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/point_get/hashmap [candidate] | wall | 9.6898 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_eq/lin [candidate] | wall | 853.1055 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_eq/sqlite [candidate] | wall | 82711.8438 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_eq/duckdb [candidate] | wall | 122175.0625 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_range/lin [candidate] | wall | 1354.9512 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_range/sqlite [candidate] | wall | 83095.8125 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/filter_range/duckdb [candidate] | wall | 293053.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/text_substr/lin [candidate] | wall | 38418.8594 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_lex_selective/lin [candidate] | wall | 852.3132 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_lex_common/lin [candidate] | wall | 136968.4375 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_lex_miss/lin [candidate] | wall | 379.8651 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/fts_hybrid_common/lin [candidate] | wall | 4717148.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/text_substr/sqlite [candidate] | wall | 300983.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/text_substr/duckdb [candidate] | wall | 142675.1875 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/materialize/lin [candidate] | wall | 798322.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/materialize/sqlite [candidate] | wall | 880375.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/materialize/duckdb [candidate] | wall | 767982.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/lin [candidate] | wall | 350172.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/lin_cursor [candidate] | wall | 1254160.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/lin_cursor_projected [candidate] | wall | 507976.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_phase/lin_cursor_open [candidate] | wall | 15492.6836 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_phase/lin_cursor_scan_project [candidate] | wall | 1429073.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_phase/lin_cursor_scan_projected_row [candidate] | wall | 876231.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/sqlite [candidate] | wall | 2460304.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_inner/duckdb [candidate] | wall | 1673249.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/lin [candidate] | wall | 184347.6250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/lin_cursor [candidate] | wall | 643296.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/sqlite [candidate] | wall | 1370139.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/join_filter/duckdb [candidate] | wall | 1089769.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum/lin [candidate] | wall | 4890.0273 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum/sqlite [candidate] | wall | 209605.5625 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum/duckdb [candidate] | wall | 66869.9688 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum_by/lin [candidate] | wall | 324765.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum_by/sqlite [candidate] | wall | 1399157.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_sum_by/duckdb [candidate] | wall | 456063.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_count_by/lin [candidate] | wall | 137901.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_count_by/sqlite [candidate] | wall | 261389.3750 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/agg_count_by/duckdb [candidate] | wall | 379594.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_1k/lin [candidate] | wall | 1101782.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_1k/sqlite [candidate] | wall | 878577.2500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_1k/duckdb [candidate] | wall | 170826538.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_10k/lin [candidate] | wall | 22744647.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_10k/sqlite [candidate] | wall | 11727604.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_bulk_10k/duckdb [candidate] | wall | 1721677553.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_native_1k/duckdb_appender [candidate] | wall | 2043479.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_native_10k/duckdb_appender [candidate] | wall | 16620943.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_full [candidate] | wall | 22364811.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_no_embed [candidate] | wall | 8499730.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_no_embed_no_scalar_index [candidate] | wall | 5522774.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts [candidate] | wall | 4131455.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_1k/lin [candidate] | wall | 218399.1250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_1k/sqlite [candidate] | wall | 447079.1250 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_1k/duckdb [candidate] | wall | 1116181.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_10k/lin [candidate] | wall | 2230585.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_10k/sqlite [candidate] | wall | 5414592.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/append_log_10k/duckdb [candidate] | wall | 8620813.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/group_commit_16/lin_sequential_full [candidate] | wall | 3237407.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/group_commit_16/lin_grouped_full [candidate] | wall | 293086.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reopen_phase_5k/rebuild_row_maps [candidate] | wall | 947983.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reopen_phase_5k/rebuild_scalar_indexes [candidate] | wall | 656493.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reopen_phase_5k/rebuild_fts [candidate] | wall | 909886.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/plan_dnf_6/lin [candidate] | wall | 134621.5625 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/plan_dnf_10/lin [candidate] | wall | 639458.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/plan_dnf_14/lin [candidate] | wall | 662204.7500 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/graph_depth_1/lin [candidate] | wall | 122785.6875 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/graph_depth_2/lin [candidate] | wall | 148663.9375 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/graph_depth_3/lin [candidate] | wall | 175696.3750 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/reader_snapshot/lin [candidate] | wall | 9708281.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |

# Useful work throughput

| Case | Median batch throughput | Unit |
|---|---:|---|
| compare/join_inner/lin [candidate] | 28557516.7409 | rows/s |
| compare/join_inner/lin_cursor [candidate] | 7973466.7368 | rows/s |
| compare/join_inner/lin_cursor_projected [candidate] | 19685959.4804 | rows/s |
| compare/join_phase/lin_cursor_scan_project [candidate] | 6997578.1394 | rows/s |
| compare/join_phase/lin_cursor_scan_projected_row [candidate] | 11412523.2277 | rows/s |
| compare/join_inner/sqlite [candidate] | 4064575.2958 | rows/s |
| compare/join_inner/duckdb [candidate] | 5976440.5648 | rows/s |
| compare/join_filter/lin [candidate] | 27122671.3755 | rows/s |
| compare/join_filter/lin_cursor [candidate] | 7772475.9217 | rows/s |
| compare/join_filter/sqlite [candidate] | 3649263.4590 | rows/s |
| compare/join_filter/duckdb [candidate] | 4588565.8046 | rows/s |
| compare/agg_sum/lin [candidate] | 2044983840.0375 | rows/s |
| compare/agg_sum/sqlite [candidate] | 47708661.2970 | rows/s |
| compare/agg_sum/duckdb [candidate] | 149597419.9685 | rows/s |
| compare/agg_sum_by/lin [candidate] | 30791424.6640 | rows/s |
| compare/agg_sum_by/sqlite [candidate] | 7147158.5525 | rows/s |
| compare/agg_sum_by/duckdb [candidate] | 21927894.6794 | rows/s |
| compare/agg_count_by/lin [candidate] | 72515468.2810 | rows/s |
| compare/agg_count_by/sqlite [candidate] | 38257106.6204 | rows/s |
| compare/agg_count_by/duckdb [candidate] | 26344051.4268 | rows/s |
| compare/insert_bulk_1k/lin [candidate] | 907640.2254 | rows/s |
| compare/insert_bulk_1k/sqlite [candidate] | 1138209.5499 | rows/s |
| compare/insert_bulk_1k/duckdb [candidate] | 5853.9024 | rows/s |
| compare/insert_bulk_10k/lin [candidate] | 439734.0657 | rows/s |
| compare/insert_bulk_10k/sqlite [candidate] | 852696.7467 | rows/s |
| compare/insert_bulk_10k/duckdb [candidate] | 5808.5625 | rows/s |
| compare/insert_native_1k/duckdb_appender [candidate] | 489364.2220 | rows/s |
| compare/insert_native_10k/duckdb_appender [candidate] | 601737.1936 | rows/s |
| compare/insert_phase_10k/lin_full [candidate] | 447131.8030 | rows/s |
| compare/insert_phase_10k/lin_no_embed [candidate] | 1176536.6156 | rows/s |
| compare/insert_phase_10k/lin_no_embed_no_scalar_index [candidate] | 1810793.6443 | rows/s |
| compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts [candidate] | 2420458.8959 | rows/s |
| compare/append_log_1k/lin [candidate] | 4578847.7069 | rows/s |
| compare/append_log_1k/sqlite [candidate] | 2236742.3962 | rows/s |
| compare/append_log_1k/duckdb [candidate] | 896008.4573 | rows/s |
| compare/append_log_10k/lin [candidate] | 4483127.9022 | rows/s |
| compare/append_log_10k/sqlite [candidate] | 1846864.6632 | rows/s |
| compare/append_log_10k/duckdb [candidate] | 1167160.5732 | rows/s |
| compare/group_commit_16/lin_sequential_full [candidate] | 4942.2559 | commits/s |
| compare/group_commit_16/lin_grouped_full [candidate] | 54592.2006 | statements/s |
| compare/reopen_phase_5k/rebuild_row_maps [candidate] | 5274353.4631 | rows/s |
| compare/reopen_phase_5k/rebuild_scalar_indexes [candidate] | 7616221.6391 | rows/s |
| compare/reopen_phase_5k/rebuild_fts [candidate] | 5495191.0547 | rows/s |

Derived from positive wall batches and declared units per operation; zero/unavailable durations omitted.

Medians above summarize observations, not pooled operation-latency percentiles. Comparisons aggregate within each process first.

- Raw batches are not independent process replications. Batch averages are not individual-operation latency. CPU frequency and background load are uncontrolled.

- compare/agg_sum_by/duckdb: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/agg_count_by/duckdb: some samples below half the requested duration; iteration cap or workload drift may limit precision

