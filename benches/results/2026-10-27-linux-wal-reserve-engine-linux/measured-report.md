# Bounded Linux WAL reservation: full-engine paired gate

All 61 cases retained. Each series has six process pairs. Positive gain means candidate faster. Targets require 6/6 positive pairs and median gain above A/A. Controls require gain >= -max(2%, A/A). Full eight-peer objective remains false.

| Case | Series | Target | Gain % | A/A % | Positive | Pass |
|---|---|---|---:|---:|---:|---|
| compare/durable_update_1row_1k/lin | series1 | True | 1.873 | 1.240 | 4/6 | False |
| compare/durable_update_1row_1k/lin | series2 | True | 5.045 | 3.568 | 6/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series1 | False | 0.366 | 3.372 | 3/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series2 | False | 0.800 | 1.483 | 4/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series1 | False | 1.486 | 5.685 | 4/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series2 | False | 1.976 | 3.030 | 5/6 | True |
| compare/durable_delete_1row_1k/lin | series1 | True | 4.041 | 3.192 | 5/6 | False |
| compare/durable_delete_1row_1k/lin | series2 | True | 3.914 | 5.616 | 5/6 | False |
| compare/durable_delete_1row_1k/sqlite_delete | series1 | False | -2.058 | 4.769 | 2/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series2 | False | -4.517 | 2.984 | 1/6 | False |
| compare/durable_delete_1row_1k/sqlite_wal | series1 | False | 1.898 | 1.803 | 4/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series2 | False | 2.908 | 3.615 | 5/6 | True |
| compare/durable_commit_1row_1k/lin | series1 | True | 0.338 | 4.869 | 3/6 | False |
| compare/durable_commit_1row_1k/lin | series2 | True | 5.378 | 5.110 | 6/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series1 | False | -2.655 | 1.599 | 2/6 | False |
| compare/durable_commit_1row_1k/sqlite_delete | series2 | False | -1.724 | 3.531 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series1 | False | -2.069 | 2.288 | 1/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series2 | False | -0.738 | 3.438 | 3/6 | True |
| compare/durable_rollback_1row_1k/lin | series1 | False | -1.957 | 2.021 | 2/6 | True |
| compare/durable_rollback_1row_1k/lin | series2 | False | 2.235 | 2.681 | 5/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series1 | False | -0.906 | 0.692 | 1/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series2 | False | 0.385 | 0.848 | 4/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series1 | False | 0.585 | 1.074 | 4/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series2 | False | 0.760 | 0.663 | 4/6 | True |
| compare/durable_update_1row_10k/lin | series1 | True | 3.895 | 5.417 | 4/6 | False |
| compare/durable_update_1row_10k/lin | series2 | True | -3.267 | 7.020 | 2/6 | False |
| compare/durable_update_1row_10k/sqlite_delete | series1 | False | 7.101 | 5.653 | 5/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series2 | False | -3.255 | 5.270 | 1/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series1 | False | -1.186 | 4.190 | 2/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series2 | False | 0.001 | 8.603 | 3/6 | True |
| compare/durable_delete_1row_10k/lin | series1 | True | 1.667 | 2.099 | 3/6 | False |
| compare/durable_delete_1row_10k/lin | series2 | True | -2.666 | 4.319 | 2/6 | False |
| compare/durable_delete_1row_10k/sqlite_delete | series1 | False | 3.426 | 7.195 | 4/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series2 | False | 0.429 | 3.907 | 3/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series1 | False | -6.237 | 3.004 | 0/6 | False |
| compare/durable_delete_1row_10k/sqlite_wal | series2 | False | -0.260 | 4.934 | 3/6 | True |
| compare/durable_commit_1row_10k/lin | series1 | True | 4.711 | 1.685 | 5/6 | False |
| compare/durable_commit_1row_10k/lin | series2 | True | -2.177 | 6.179 | 3/6 | False |
| compare/durable_commit_1row_10k/sqlite_delete | series1 | False | -0.391 | 5.352 | 2/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series2 | False | -1.067 | 4.330 | 2/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series1 | False | 0.566 | 7.728 | 3/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series2 | False | 2.885 | 5.577 | 3/6 | True |
| compare/durable_rollback_1row_10k/lin | series1 | False | 0.975 | 0.847 | 4/6 | True |
| compare/durable_rollback_1row_10k/lin | series2 | False | 1.051 | 2.400 | 4/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series1 | False | 0.607 | 0.837 | 4/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series2 | False | 1.285 | 3.691 | 5/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series1 | False | -1.200 | 1.236 | 2/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series2 | False | -0.531 | 1.002 | 2/6 | True |
| compare/update_1row_1k/lin | series1 | False | 0.786 | 1.375 | 4/6 | True |
| compare/update_1row_1k/lin | series2 | False | -1.562 | 2.156 | 1/6 | True |
| compare/update_1row_1k/sqlite | series1 | False | 3.440 | 1.878 | 6/6 | True |
| compare/update_1row_1k/sqlite | series2 | False | -0.334 | 1.974 | 3/6 | True |
| compare/delete_1row_1k/lin | series1 | False | 1.233 | 1.707 | 4/6 | True |
| compare/delete_1row_1k/lin | series2 | False | 0.303 | 1.161 | 3/6 | True |
| compare/delete_1row_1k/sqlite | series1 | False | -0.872 | 3.581 | 3/6 | True |
| compare/delete_1row_1k/sqlite | series2 | False | 2.505 | 2.586 | 5/6 | True |
| compare/update_1row_10k/lin | series1 | False | 0.119 | 1.620 | 3/6 | True |
| compare/update_1row_10k/lin | series2 | False | 1.450 | 1.632 | 3/6 | True |
| compare/update_1row_10k/sqlite | series1 | False | -4.200 | 4.194 | 1/6 | False |
| compare/update_1row_10k/sqlite | series2 | False | 1.746 | 2.683 | 4/6 | True |
| compare/delete_1row_10k/lin | series1 | False | -0.170 | 1.868 | 2/6 | True |
| compare/delete_1row_10k/lin | series2 | False | 3.046 | 1.749 | 5/6 | True |
| compare/delete_1row_10k/sqlite | series1 | False | 3.667 | 2.769 | 4/6 | True |
| compare/delete_1row_10k/sqlite | series2 | False | 1.603 | 3.690 | 4/6 | True |
| compare/update_1row_100k/lin | series1 | False | -2.941 | 5.909 | 2/6 | True |
| compare/update_1row_100k/lin | series2 | False | -0.521 | 0.933 | 3/6 | True |
| compare/update_1row_100k/sqlite | series1 | False | 1.034 | 5.338 | 3/6 | True |
| compare/update_1row_100k/sqlite | series2 | False | -1.909 | 4.268 | 2/6 | True |
| compare/delete_1row_100k/lin | series1 | False | 1.150 | 0.996 | 4/6 | True |
| compare/delete_1row_100k/lin | series2 | False | -0.273 | 2.842 | 2/6 | True |
| compare/delete_1row_100k/sqlite | series1 | False | 0.635 | 2.529 | 3/6 | True |
| compare/delete_1row_100k/sqlite | series2 | False | 1.921 | 1.596 | 3/6 | True |
| compare/tx_commit_1row_1k/lin | series1 | False | -1.371 | 1.969 | 2/6 | True |
| compare/tx_commit_1row_1k/lin | series2 | False | -0.005 | 2.093 | 3/6 | True |
| compare/tx_commit_1row_1k/sqlite | series1 | False | -0.282 | 1.410 | 2/6 | True |
| compare/tx_commit_1row_1k/sqlite | series2 | False | 0.337 | 4.478 | 3/6 | True |
| compare/tx_rollback_1row_1k/lin | series1 | False | 0.837 | 2.229 | 4/6 | True |
| compare/tx_rollback_1row_1k/lin | series2 | False | -2.039 | 1.701 | 2/6 | False |
| compare/tx_rollback_1row_1k/sqlite | series1 | False | 3.575 | 2.028 | 4/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series2 | False | 2.934 | 4.556 | 5/6 | True |
| compare/tx_commit_1row_10k/lin | series1 | False | 1.691 | 0.988 | 4/6 | True |
| compare/tx_commit_1row_10k/lin | series2 | False | 0.376 | 2.065 | 3/6 | True |
| compare/tx_commit_1row_10k/sqlite | series1 | False | 1.793 | 2.496 | 4/6 | True |
| compare/tx_commit_1row_10k/sqlite | series2 | False | 5.637 | 1.917 | 4/6 | True |
| compare/tx_rollback_1row_10k/lin | series1 | False | 1.007 | 0.370 | 3/6 | True |
| compare/tx_rollback_1row_10k/lin | series2 | False | 2.185 | 1.759 | 4/6 | True |
| compare/tx_rollback_1row_10k/sqlite | series1 | False | 3.458 | 3.972 | 5/6 | True |
| compare/tx_rollback_1row_10k/sqlite | series2 | False | 1.162 | 3.300 | 3/6 | True |
| compare/insert_bulk_1k/lin | series1 | False | 0.110 | 0.639 | 3/6 | True |
| compare/insert_bulk_1k/lin | series2 | False | -0.181 | 0.588 | 1/6 | True |
| compare/insert_bulk_1k/sqlite | series1 | False | 0.435 | 0.818 | 3/6 | True |
| compare/insert_bulk_1k/sqlite | series2 | False | 0.891 | 1.020 | 5/6 | True |
| compare/insert_bulk_10k/lin | series1 | False | -0.612 | 45.402 | 1/6 | True |
| compare/insert_bulk_10k/lin | series2 | False | -0.318 | 46.064 | 2/6 | True |
| compare/insert_bulk_10k/sqlite | series1 | False | 0.911 | 0.512 | 5/6 | True |
| compare/insert_bulk_10k/sqlite | series2 | False | 0.396 | 0.320 | 5/6 | True |
| compare/durable_insert_1k/lin | series1 | False | 2.276 | 0.571 | 5/6 | True |
| compare/durable_insert_1k/lin | series2 | False | 4.095 | 1.727 | 5/6 | True |
| compare/durable_insert_1k/sqlite | series1 | False | 0.569 | 2.235 | 4/6 | True |
| compare/durable_insert_1k/sqlite | series2 | False | 1.453 | 2.696 | 4/6 | True |
| compare/durable_insert_10k/lin | series1 | False | 0.865 | 0.890 | 5/6 | True |
| compare/durable_insert_10k/lin | series2 | False | 0.526 | 0.648 | 4/6 | True |
| compare/durable_insert_10k/sqlite | series1 | False | 0.349 | 0.385 | 4/6 | True |
| compare/durable_insert_10k/sqlite | series2 | False | 0.169 | 0.438 | 4/6 | True |
| compare/filter_eq/lin | series1 | False | -2.146 | 2.123 | 2/6 | False |
| compare/filter_eq/lin | series2 | False | 1.248 | 2.443 | 3/6 | True |
| compare/filter_eq/sqlite | series1 | False | -0.233 | 0.542 | 2/6 | True |
| compare/filter_eq/sqlite | series2 | False | -1.915 | 1.014 | 1/6 | True |
| compare/filter_eq/duckdb | series1 | False | -0.486 | 3.563 | 3/6 | True |
| compare/filter_eq/duckdb | series2 | False | 2.371 | 4.134 | 4/6 | True |
| compare/filter_range/lin | series1 | False | -1.268 | 3.792 | 2/6 | True |
| compare/filter_range/lin | series2 | False | -1.654 | 1.552 | 0/6 | True |
| compare/filter_range/sqlite | series1 | False | -0.496 | 0.378 | 2/6 | True |
| compare/filter_range/sqlite | series2 | False | -0.358 | 1.004 | 2/6 | True |
| compare/filter_range/duckdb | series1 | False | 0.286 | 2.372 | 4/6 | True |
| compare/filter_range/duckdb | series2 | False | -0.626 | 4.334 | 2/6 | True |
| compare/agg_count_by/lin | series1 | False | 0.513 | 0.470 | 5/6 | True |
| compare/agg_count_by/lin | series2 | False | 0.918 | 1.585 | 4/6 | True |
| compare/agg_count_by/sqlite | series1 | False | -4.842 | 1.649 | 0/6 | False |
| compare/agg_count_by/sqlite | series2 | False | -2.644 | 0.945 | 0/6 | False |
| compare/agg_count_by/duckdb | series1 | False | -1.896 | 3.173 | 3/6 | True |
| compare/agg_count_by/duckdb | series2 | False | 1.105 | 4.143 | 3/6 | True |

| Lin case | Peer | Series | Lin/peer | Lin wins |
|---|---|---|---:|---:|
| compare/durable_update_1row_1k/lin | sqlite_delete | series1 | 0.4009 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series1 | 1.1993 | 0/6 |
| compare/durable_update_1row_1k/lin | sqlite_delete | series2 | 0.3863 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series2 | 1.1370 | 0/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series1 | 0.3964 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series1 | 1.1691 | 0/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series2 | 0.3781 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series2 | 1.1726 | 0/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series1 | 0.3970 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series1 | 1.1704 | 0/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series2 | 0.3989 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series2 | 1.1591 | 0/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series1 | 0.2321 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series1 | 1.3710 | 0/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series2 | 0.2274 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series2 | 1.3533 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series1 | 0.6208 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series1 | 1.7977 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series2 | 0.6532 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series2 | 1.8953 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series1 | 0.6340 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series1 | 1.6686 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series2 | 0.6694 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series2 | 1.8210 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series1 | 0.5999 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series1 | 1.7583 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series2 | 0.6216 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series2 | 1.8181 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series1 | 0.3898 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series1 | 2.0439 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series2 | 0.3825 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series2 | 2.0310 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series1 | 2.0917 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series2 | 2.1027 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series1 | 2.5848 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series2 | 2.5587 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series1 | 2.2373 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series2 | 2.2832 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series1 | 2.6007 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series2 | 2.4979 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series1 | 2.1895 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series2 | 2.0784 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series1 | 2.4880 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series2 | 2.4824 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series1 | 1.6563 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series2 | 1.6671 | 0/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series1 | 2.1043 | 0/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series2 | 2.2310 | 0/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series1 | 1.8263 | 0/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series2 | 1.8554 | 0/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series1 | 2.2094 | 0/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series2 | 2.1346 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series1 | 1.1495 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series2 | 1.1583 | 0/6 |
| compare/insert_bulk_10k/lin | sqlite | series1 | 1.8640 | 0/6 |
| compare/insert_bulk_10k/lin | sqlite | series2 | 1.8706 | 0/6 |
| compare/durable_insert_1k/lin | sqlite | series1 | 0.8602 | 6/6 |
| compare/durable_insert_1k/lin | sqlite | series2 | 0.8484 | 6/6 |
| compare/durable_insert_10k/lin | sqlite | series1 | 1.7590 | 0/6 |
| compare/durable_insert_10k/lin | sqlite | series2 | 1.7519 | 0/6 |
| compare/filter_eq/lin | sqlite | series1 | 0.0097 | 6/6 |
| compare/filter_eq/lin | duckdb | series1 | 0.0058 | 6/6 |
| compare/filter_eq/lin | sqlite | series2 | 0.0095 | 6/6 |
| compare/filter_eq/lin | duckdb | series2 | 0.0058 | 6/6 |
| compare/filter_range/lin | sqlite | series1 | 0.0144 | 6/6 |
| compare/filter_range/lin | duckdb | series1 | 0.0048 | 6/6 |
| compare/filter_range/lin | sqlite | series2 | 0.0145 | 6/6 |
| compare/filter_range/lin | duckdb | series2 | 0.0048 | 6/6 |
| compare/agg_count_by/lin | sqlite | series1 | 0.2785 | 6/6 |
| compare/agg_count_by/lin | duckdb | series1 | 0.2066 | 6/6 |
| compare/agg_count_by/lin | sqlite | series2 | 0.2787 | 6/6 |
| compare/agg_count_by/lin | duckdb | series2 | 0.2122 | 6/6 |

Source gate: False. Numeric peer wins: 180/432. Whole-process RSS in summary.json includes fixture, oracle and reopen; it is not isolated operation memory.
