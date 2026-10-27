# Direct Batch MessagePack WAL: full-engine paired gate

All 63 cases retained. Each series has six process pairs. Positive gain means candidate faster. Targets require 6/6 positive pairs and median gain above A/A. Controls require gain >= -max(2%, A/A). Full eight-peer objective remains false.

| Case | Series | Target | Gain % | A/A % | Positive | Pass |
|---|---|---|---:|---:|---:|---|
| compare/durable_update_1row_1k/lin | series1 | True | 13.431 | 4.486 | 6/6 | True |
| compare/durable_update_1row_1k/lin | series2 | True | 9.933 | 2.122 | 6/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series1 | False | -1.281 | 8.383 | 2/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series2 | False | 1.990 | 3.483 | 4/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series1 | False | 4.727 | 3.345 | 5/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series2 | False | -2.586 | 2.818 | 2/6 | True |
| compare/durable_delete_1row_1k/lin | series1 | True | 10.380 | 3.764 | 6/6 | True |
| compare/durable_delete_1row_1k/lin | series2 | True | 6.519 | 6.502 | 6/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series1 | False | -0.484 | 2.853 | 3/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series2 | False | 4.324 | 3.289 | 5/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series1 | False | 3.797 | 5.469 | 5/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series2 | False | 4.158 | 6.747 | 5/6 | True |
| compare/durable_commit_1row_1k/lin | series1 | True | 16.320 | 4.498 | 6/6 | True |
| compare/durable_commit_1row_1k/lin | series2 | True | 9.808 | 1.851 | 6/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series1 | False | -0.105 | 2.336 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series2 | False | -1.736 | 2.387 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series1 | False | -1.226 | 2.759 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series2 | False | 2.514 | 7.988 | 5/6 | True |
| compare/durable_rollback_1row_1k/lin | series1 | False | 1.234 | 2.136 | 4/6 | True |
| compare/durable_rollback_1row_1k/lin | series2 | False | -0.545 | 0.928 | 2/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series1 | False | -0.620 | 2.330 | 2/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series2 | False | -3.988 | 5.804 | 2/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series1 | False | -1.054 | 6.793 | 2/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series2 | False | 2.113 | 3.043 | 5/6 | True |
| compare/durable_update_1row_10k/lin | series1 | True | 13.124 | 8.647 | 5/6 | False |
| compare/durable_update_1row_10k/lin | series2 | True | 7.020 | 4.389 | 5/6 | False |
| compare/durable_update_1row_10k/sqlite_delete | series1 | False | -1.743 | 7.479 | 2/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series2 | False | 4.219 | 5.375 | 4/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series1 | False | -2.894 | 6.142 | 2/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series2 | False | -0.726 | 3.916 | 3/6 | True |
| compare/durable_delete_1row_10k/lin | series1 | True | 16.573 | 4.382 | 6/6 | True |
| compare/durable_delete_1row_10k/lin | series2 | True | 11.591 | 4.464 | 6/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series1 | False | -0.998 | 5.866 | 3/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series2 | False | 2.517 | 5.089 | 5/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series1 | False | 1.755 | 4.253 | 4/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series2 | False | 1.018 | 3.783 | 3/6 | True |
| compare/durable_commit_1row_10k/lin | series1 | True | 21.013 | 9.323 | 6/6 | True |
| compare/durable_commit_1row_10k/lin | series2 | True | 21.682 | 4.677 | 6/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series1 | False | 0.944 | 1.574 | 3/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series2 | False | -0.309 | 5.492 | 3/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series1 | False | -1.664 | 1.390 | 3/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series2 | False | 3.463 | 3.866 | 6/6 | True |
| compare/durable_rollback_1row_10k/lin | series1 | False | 0.426 | 3.750 | 3/6 | True |
| compare/durable_rollback_1row_10k/lin | series2 | False | 2.861 | 2.661 | 5/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series1 | False | -0.884 | 1.623 | 2/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series2 | False | 0.193 | 2.164 | 4/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series1 | False | -2.685 | 4.250 | 2/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series2 | False | 0.745 | 4.368 | 3/6 | True |
| compare/update_1row_1k/lin | series1 | False | -1.355 | 2.869 | 2/6 | True |
| compare/update_1row_1k/lin | series2 | False | 0.940 | 4.090 | 3/6 | True |
| compare/update_1row_1k/sqlite | series1 | False | 4.721 | 5.595 | 5/6 | True |
| compare/update_1row_1k/sqlite | series2 | False | 1.442 | 2.747 | 5/6 | True |
| compare/delete_1row_1k/lin | series1 | False | -3.412 | 3.889 | 2/6 | True |
| compare/delete_1row_1k/lin | series2 | False | 2.801 | 3.481 | 5/6 | True |
| compare/delete_1row_1k/sqlite | series1 | False | -1.039 | 4.834 | 1/6 | True |
| compare/delete_1row_1k/sqlite | series2 | False | 2.412 | 3.422 | 5/6 | True |
| compare/update_1row_10k/lin | series1 | False | 0.774 | 3.058 | 3/6 | True |
| compare/update_1row_10k/lin | series2 | False | -0.577 | 3.983 | 3/6 | True |
| compare/update_1row_10k/sqlite | series1 | False | 3.445 | 6.431 | 4/6 | True |
| compare/update_1row_10k/sqlite | series2 | False | 2.252 | 4.547 | 4/6 | True |
| compare/delete_1row_10k/lin | series1 | False | 0.969 | 2.455 | 4/6 | True |
| compare/delete_1row_10k/lin | series2 | False | -4.783 | 3.273 | 1/6 | False |
| compare/delete_1row_10k/sqlite | series1 | False | -1.119 | 6.822 | 3/6 | True |
| compare/delete_1row_10k/sqlite | series2 | False | -3.789 | 1.390 | 2/6 | False |
| compare/update_1row_100k/lin | series1 | False | 2.987 | 3.681 | 5/6 | True |
| compare/update_1row_100k/lin | series2 | False | -0.849 | 1.916 | 3/6 | True |
| compare/update_1row_100k/sqlite | series1 | False | 2.679 | 1.797 | 6/6 | True |
| compare/update_1row_100k/sqlite | series2 | False | 1.248 | 1.763 | 4/6 | True |
| compare/delete_1row_100k/lin | series1 | False | 1.916 | 2.471 | 4/6 | True |
| compare/delete_1row_100k/lin | series2 | False | -0.156 | 1.777 | 3/6 | True |
| compare/delete_1row_100k/sqlite | series1 | False | -0.969 | 3.006 | 2/6 | True |
| compare/delete_1row_100k/sqlite | series2 | False | 0.283 | 1.701 | 3/6 | True |
| compare/tx_commit_1row_1k/lin | series1 | False | 0.970 | 5.273 | 3/6 | True |
| compare/tx_commit_1row_1k/lin | series2 | False | 0.211 | 1.719 | 3/6 | True |
| compare/tx_commit_1row_1k/sqlite | series1 | False | 2.799 | 5.468 | 6/6 | True |
| compare/tx_commit_1row_1k/sqlite | series2 | False | 2.180 | 3.213 | 4/6 | True |
| compare/tx_rollback_1row_1k/lin | series1 | False | -0.953 | 1.337 | 3/6 | True |
| compare/tx_rollback_1row_1k/lin | series2 | False | -0.459 | 2.641 | 3/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series1 | False | -3.326 | 7.786 | 2/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series2 | False | 1.787 | 3.112 | 3/6 | True |
| compare/tx_commit_1row_10k/lin | series1 | False | -3.257 | 3.612 | 1/6 | True |
| compare/tx_commit_1row_10k/lin | series2 | False | -5.030 | 3.489 | 1/6 | False |
| compare/tx_commit_1row_10k/sqlite | series1 | False | -1.674 | 3.710 | 3/6 | True |
| compare/tx_commit_1row_10k/sqlite | series2 | False | 3.133 | 2.635 | 5/6 | True |
| compare/tx_rollback_1row_10k/lin | series1 | False | -0.188 | 0.496 | 3/6 | True |
| compare/tx_rollback_1row_10k/lin | series2 | False | -6.041 | 3.996 | 2/6 | False |
| compare/tx_rollback_1row_10k/sqlite | series1 | False | 3.078 | 4.419 | 4/6 | True |
| compare/tx_rollback_1row_10k/sqlite | series2 | False | -0.032 | 1.890 | 3/6 | True |
| compare/insert_bulk_1k/lin | series1 | False | 1.095 | 4.310 | 3/6 | True |
| compare/insert_bulk_1k/lin | series2 | False | 0.022 | 1.475 | 3/6 | True |
| compare/insert_bulk_1k/sqlite | series1 | False | -5.936 | 3.633 | 2/6 | False |
| compare/insert_bulk_1k/sqlite | series2 | False | 1.664 | 1.366 | 6/6 | True |
| compare/insert_bulk_10k/lin | series1 | False | 1.634 | 2.810 | 4/6 | True |
| compare/insert_bulk_10k/lin | series2 | False | 2.826 | 1.826 | 5/6 | True |
| compare/insert_bulk_10k/sqlite | series1 | False | -0.575 | 1.270 | 1/6 | True |
| compare/insert_bulk_10k/sqlite | series2 | False | 0.050 | 0.447 | 3/6 | True |
| compare/durable_insert_1k/lin | series1 | False | -2.551 | 2.867 | 2/6 | True |
| compare/durable_insert_1k/lin | series2 | False | 2.049 | 1.372 | 5/6 | True |
| compare/durable_insert_1k/sqlite | series1 | False | 0.279 | 3.111 | 3/6 | True |
| compare/durable_insert_1k/sqlite | series2 | False | -1.226 | 1.584 | 3/6 | True |
| compare/durable_insert_10k/lin | series1 | False | 0.853 | 0.951 | 3/6 | True |
| compare/durable_insert_10k/lin | series2 | False | 0.047 | 2.245 | 3/6 | True |
| compare/durable_insert_10k/sqlite | series1 | False | -0.431 | 0.955 | 2/6 | True |
| compare/durable_insert_10k/sqlite | series2 | False | -0.306 | 0.892 | 3/6 | True |
| compare/filter_eq/lin | series1 | False | -3.425 | 2.138 | 2/6 | False |
| compare/filter_eq/lin | series2 | False | 0.115 | 2.760 | 3/6 | True |
| compare/filter_eq/sqlite | series1 | False | 2.014 | 2.579 | 4/6 | True |
| compare/filter_eq/sqlite | series2 | False | 2.013 | 2.519 | 4/6 | True |
| compare/filter_eq/duckdb | series1 | False | 3.240 | 6.298 | 4/6 | True |
| compare/filter_eq/duckdb | series2 | False | 1.844 | 7.559 | 3/6 | True |
| compare/filter_range/lin | series1 | False | 6.470 | 2.178 | 5/6 | True |
| compare/filter_range/lin | series2 | False | -5.640 | 2.362 | 1/6 | False |
| compare/filter_range/sqlite | series1 | False | -2.739 | 2.329 | 1/6 | False |
| compare/filter_range/sqlite | series2 | False | 2.922 | 5.512 | 4/6 | True |
| compare/filter_range/duckdb | series1 | False | -3.216 | 3.803 | 2/6 | True |
| compare/filter_range/duckdb | series2 | False | -1.232 | 4.757 | 3/6 | True |
| compare/agg_count_by/lin | series1 | False | 2.807 | 6.223 | 5/6 | True |
| compare/agg_count_by/lin | series2 | False | -0.954 | 1.198 | 2/6 | True |
| compare/agg_count_by/sqlite | series1 | False | 5.364 | 8.440 | 5/6 | True |
| compare/agg_count_by/sqlite | series2 | False | -3.045 | 3.916 | 1/6 | True |
| compare/agg_count_by/duckdb | series1 | False | -3.529 | 6.261 | 2/6 | True |
| compare/agg_count_by/duckdb | series2 | False | -1.945 | 9.216 | 2/6 | True |
| compare/wal_scalar_full_update/lin | series1 | False | -0.455 | 4.622 | 3/6 | True |
| compare/wal_scalar_full_update/lin | series2 | False | -1.667 | 1.717 | 3/6 | True |
| compare/wal_scalar_normal_update/lin | series1 | False | 0.133 | 1.785 | 4/6 | True |
| compare/wal_scalar_normal_update/lin | series2 | False | -2.739 | 4.182 | 2/6 | True |

| Lin case | Peer | Series | Lin/peer | Lin wins |
|---|---|---|---:|---:|
| compare/durable_update_1row_1k/lin | sqlite_delete | series1 | 0.3159 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series1 | 1.0041 | 3/6 |
| compare/durable_update_1row_1k/lin | sqlite_delete | series2 | 0.3504 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series2 | 1.0524 | 1/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series1 | 0.3363 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series1 | 1.0446 | 1/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series2 | 0.3621 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series2 | 1.0804 | 1/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series1 | 0.3335 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series1 | 0.9758 | 4/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series2 | 0.3550 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series2 | 1.0549 | 1/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series1 | 0.4370 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series1 | 2.0701 | 0/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series2 | 0.4430 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series2 | 1.9495 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series1 | 0.4114 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series1 | 1.2526 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series2 | 0.4229 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series2 | 1.2229 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series1 | 0.4288 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series1 | 1.2992 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series2 | 0.4332 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series2 | 1.2737 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series1 | 0.4414 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series1 | 1.2461 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series2 | 0.4484 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series2 | 1.2713 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series1 | 0.5274 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series1 | 2.1315 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series2 | 0.4630 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series2 | 1.7898 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series1 | 2.6778 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series2 | 2.6378 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series1 | 3.2441 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series2 | 3.2611 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series1 | 2.3966 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series2 | 2.2767 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series1 | 2.8937 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series2 | 2.8557 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series1 | 1.8089 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series2 | 1.5944 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series1 | 2.1031 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series2 | 2.0321 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series1 | 2.1249 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series2 | 2.1609 | 0/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series1 | 2.5346 | 0/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series2 | 2.6917 | 0/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series1 | 2.0139 | 0/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series2 | 1.9809 | 0/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series1 | 2.4534 | 0/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series2 | 2.5607 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series1 | 1.1048 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series2 | 1.1351 | 0/6 |
| compare/insert_bulk_10k/lin | sqlite | series1 | 1.6176 | 0/6 |
| compare/insert_bulk_10k/lin | sqlite | series2 | 1.5864 | 0/6 |
| compare/durable_insert_1k/lin | sqlite | series1 | 1.0900 | 0/6 |
| compare/durable_insert_1k/lin | sqlite | series2 | 1.1708 | 0/6 |
| compare/durable_insert_10k/lin | sqlite | series1 | 1.4097 | 0/6 |
| compare/durable_insert_10k/lin | sqlite | series2 | 1.3666 | 0/6 |
| compare/filter_eq/lin | sqlite | series1 | 0.0117 | 6/6 |
| compare/filter_eq/lin | duckdb | series1 | 0.0058 | 6/6 |
| compare/filter_eq/lin | sqlite | series2 | 0.0113 | 6/6 |
| compare/filter_eq/lin | duckdb | series2 | 0.0058 | 6/6 |
| compare/filter_range/lin | sqlite | series1 | 0.0161 | 6/6 |
| compare/filter_range/lin | duckdb | series1 | 0.0043 | 6/6 |
| compare/filter_range/lin | sqlite | series2 | 0.0175 | 6/6 |
| compare/filter_range/lin | duckdb | series2 | 0.0045 | 6/6 |
| compare/agg_count_by/lin | sqlite | series1 | 0.5320 | 6/6 |
| compare/agg_count_by/lin | duckdb | series1 | 0.3224 | 6/6 |
| compare/agg_count_by/lin | sqlite | series2 | 0.5219 | 6/6 |
| compare/agg_count_by/lin | duckdb | series2 | 0.3298 | 6/6 |

Source gate: False. Numeric peer wins: 179/432. Whole-process RSS in summary.json includes fixture, oracle and reopen; it is not isolated operation memory.

The frozen generator retained an old display heading and CPU descriptive flag. Both are labeling errors: the independently recomputed CPU gate for this exact helper passed. Full-engine latency acceptance is computed solely from raw targets/controls and is not changed by this correction. Original report and qualification JSON remain unchanged in the archived artifact.
