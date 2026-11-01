# Corrected COW + CAS + UPDATE: full-engine paired gate

All 67 cases retained. Each series has six process pairs. Positive gain means candidate faster. Targets require 6/6 positive pairs and median gain above A/A. Controls require gain >= -max(2%, A/A). Full eight-peer objective remains false.

| Case | Series | Target | Gain % | A/A % | Positive | Pass |
|---|---|---|---:|---:|---:|---|
| compare/durable_update_1row_1k/lin | series1 | False | -6.366 | 7.092 | 2/6 | True |
| compare/durable_update_1row_1k/lin | series2 | False | 0.128 | 8.073 | 3/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series1 | False | -4.456 | 8.142 | 1/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series2 | False | 6.204 | 9.817 | 4/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series1 | False | -1.111 | 4.885 | 2/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series2 | False | -2.762 | 5.912 | 1/6 | True |
| compare/durable_delete_1row_1k/lin | series1 | False | -0.589 | 7.996 | 3/6 | True |
| compare/durable_delete_1row_1k/lin | series2 | False | 5.032 | 7.987 | 4/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series1 | False | 2.595 | 5.547 | 3/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series2 | False | 1.410 | 6.948 | 4/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series1 | False | 2.687 | 6.010 | 4/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series2 | False | -2.209 | 3.136 | 3/6 | True |
| compare/durable_commit_1row_1k/lin | series1 | False | 11.278 | 5.629 | 5/6 | True |
| compare/durable_commit_1row_1k/lin | series2 | False | -6.792 | 6.343 | 2/6 | False |
| compare/durable_commit_1row_1k/sqlite_delete | series1 | False | -1.004 | 6.809 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series2 | False | 6.263 | 6.858 | 5/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series1 | False | -3.118 | 6.608 | 2/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series2 | False | -1.745 | 6.568 | 3/6 | True |
| compare/durable_rollback_1row_1k/lin | series1 | False | 17.092 | 7.985 | 6/6 | True |
| compare/durable_rollback_1row_1k/lin | series2 | False | 13.532 | 8.682 | 5/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series1 | False | 1.227 | 0.845 | 4/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series2 | False | -0.926 | 1.302 | 1/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series1 | False | 0.015 | 2.155 | 3/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series2 | False | 0.570 | 2.545 | 4/6 | True |
| compare/durable_update_1row_10k/lin | series1 | False | 3.569 | 5.117 | 4/6 | True |
| compare/durable_update_1row_10k/lin | series2 | False | -0.367 | 3.409 | 3/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series1 | False | -2.427 | 9.426 | 3/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series2 | False | 0.093 | 7.950 | 3/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series1 | False | 4.245 | 9.542 | 5/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series2 | False | -3.045 | 7.743 | 1/6 | True |
| compare/durable_delete_1row_10k/lin | series1 | False | -2.127 | 4.281 | 1/6 | True |
| compare/durable_delete_1row_10k/lin | series2 | False | -1.715 | 2.481 | 1/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series1 | False | -10.839 | 4.965 | 0/6 | False |
| compare/durable_delete_1row_10k/sqlite_delete | series2 | False | -5.533 | 13.274 | 1/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series1 | False | 0.516 | 3.844 | 3/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series2 | False | 2.841 | 6.305 | 3/6 | True |
| compare/durable_commit_1row_10k/lin | series1 | False | 2.041 | 4.222 | 5/6 | True |
| compare/durable_commit_1row_10k/lin | series2 | False | 1.222 | 7.495 | 5/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series1 | False | 7.498 | 4.342 | 4/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series2 | False | 3.774 | 11.259 | 4/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series1 | False | 1.699 | 4.912 | 4/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series2 | False | 2.114 | 7.672 | 3/6 | True |
| compare/durable_rollback_1row_10k/lin | series1 | False | 14.813 | 2.918 | 6/6 | True |
| compare/durable_rollback_1row_10k/lin | series2 | False | 13.530 | 1.894 | 6/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series1 | False | -0.117 | 5.622 | 3/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series2 | False | -0.022 | 4.145 | 3/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series1 | False | -0.021 | 9.578 | 3/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series2 | False | 2.912 | 8.442 | 5/6 | True |
| compare/update_1row_1k/lin | series1 | True | 55.189 | 3.164 | 6/6 | True |
| compare/update_1row_1k/lin | series2 | True | 56.261 | 1.778 | 6/6 | True |
| compare/update_1row_1k/sqlite | series1 | False | 0.063 | 5.691 | 3/6 | True |
| compare/update_1row_1k/sqlite | series2 | False | 2.245 | 5.582 | 4/6 | True |
| compare/delete_1row_1k/lin | series1 | False | -3.199 | 6.957 | 2/6 | True |
| compare/delete_1row_1k/lin | series2 | False | 6.332 | 3.611 | 4/6 | True |
| compare/delete_1row_1k/sqlite | series1 | False | -5.859 | 7.766 | 1/6 | True |
| compare/delete_1row_1k/sqlite | series2 | False | -1.091 | 5.496 | 2/6 | True |
| compare/update_1row_10k/lin | series1 | True | 53.396 | 4.479 | 6/6 | True |
| compare/update_1row_10k/lin | series2 | True | 50.899 | 3.130 | 6/6 | True |
| compare/update_1row_10k/sqlite | series1 | False | 5.544 | 3.662 | 5/6 | True |
| compare/update_1row_10k/sqlite | series2 | False | -1.505 | 6.397 | 1/6 | True |
| compare/delete_1row_10k/lin | series1 | False | 2.923 | 2.341 | 5/6 | True |
| compare/delete_1row_10k/lin | series2 | False | 0.738 | 2.620 | 5/6 | True |
| compare/delete_1row_10k/sqlite | series1 | False | 3.749 | 5.658 | 4/6 | True |
| compare/delete_1row_10k/sqlite | series2 | False | 1.641 | 4.103 | 4/6 | True |
| compare/update_1row_100k/lin | series1 | True | 52.950 | 2.608 | 6/6 | True |
| compare/update_1row_100k/lin | series2 | True | 56.828 | 4.203 | 6/6 | True |
| compare/update_1row_100k/sqlite | series1 | False | 2.667 | 15.123 | 4/6 | True |
| compare/update_1row_100k/sqlite | series2 | False | 18.505 | 9.867 | 5/6 | True |
| compare/delete_1row_100k/lin | series1 | False | 4.949 | 2.657 | 5/6 | True |
| compare/delete_1row_100k/lin | series2 | False | 1.659 | 8.809 | 3/6 | True |
| compare/delete_1row_100k/sqlite | series1 | False | 4.506 | 4.169 | 4/6 | True |
| compare/delete_1row_100k/sqlite | series2 | False | 0.249 | 9.798 | 3/6 | True |
| compare/tx_commit_1row_1k/lin | series1 | False | 50.718 | 5.079 | 6/6 | True |
| compare/tx_commit_1row_1k/lin | series2 | False | 55.118 | 8.789 | 6/6 | True |
| compare/tx_commit_1row_1k/sqlite | series1 | False | 1.714 | 7.266 | 3/6 | True |
| compare/tx_commit_1row_1k/sqlite | series2 | False | 0.076 | 7.962 | 3/6 | True |
| compare/tx_rollback_1row_1k/lin | series1 | False | 42.385 | 6.634 | 6/6 | True |
| compare/tx_rollback_1row_1k/lin | series2 | False | 41.083 | 3.797 | 6/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series1 | False | -0.727 | 3.284 | 3/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series2 | False | -0.724 | 3.108 | 3/6 | True |
| compare/tx_commit_1row_10k/lin | series1 | False | 50.772 | 2.681 | 6/6 | True |
| compare/tx_commit_1row_10k/lin | series2 | False | 50.503 | 2.520 | 6/6 | True |
| compare/tx_commit_1row_10k/sqlite | series1 | False | -2.879 | 3.176 | 2/6 | True |
| compare/tx_commit_1row_10k/sqlite | series2 | False | -2.873 | 5.829 | 2/6 | True |
| compare/tx_rollback_1row_10k/lin | series1 | False | 37.326 | 2.073 | 6/6 | True |
| compare/tx_rollback_1row_10k/lin | series2 | False | 38.432 | 2.261 | 6/6 | True |
| compare/tx_rollback_1row_10k/sqlite | series1 | False | 0.219 | 4.195 | 3/6 | True |
| compare/tx_rollback_1row_10k/sqlite | series2 | False | 2.278 | 4.275 | 4/6 | True |
| compare/insert_bulk_1k/lin | series1 | False | 1.206 | 1.796 | 4/6 | True |
| compare/insert_bulk_1k/lin | series2 | False | -1.179 | 2.458 | 2/6 | True |
| compare/insert_bulk_1k/sqlite | series1 | False | -0.251 | 1.486 | 2/6 | True |
| compare/insert_bulk_1k/sqlite | series2 | False | -0.163 | 0.746 | 3/6 | True |
| compare/insert_bulk_10k/lin | series1 | False | 44.465 | 2.180 | 6/6 | True |
| compare/insert_bulk_10k/lin | series2 | False | 43.859 | 2.233 | 6/6 | True |
| compare/insert_bulk_10k/sqlite | series1 | False | -0.354 | 0.661 | 1/6 | True |
| compare/insert_bulk_10k/sqlite | series2 | False | -0.113 | 0.705 | 2/6 | True |
| compare/durable_insert_1k/lin | series1 | False | -0.714 | 5.090 | 3/6 | True |
| compare/durable_insert_1k/lin | series2 | False | -1.125 | 3.922 | 2/6 | True |
| compare/durable_insert_1k/sqlite | series1 | False | -1.746 | 2.840 | 1/6 | True |
| compare/durable_insert_1k/sqlite | series2 | False | 2.045 | 3.834 | 5/6 | True |
| compare/durable_insert_10k/lin | series1 | False | 1.102 | 56.553 | 3/6 | True |
| compare/durable_insert_10k/lin | series2 | False | 18.358 | 5.422 | 5/6 | True |
| compare/durable_insert_10k/sqlite | series1 | False | -1.591 | 1.160 | 0/6 | True |
| compare/durable_insert_10k/sqlite | series2 | False | -0.901 | 0.824 | 2/6 | True |
| compare/filter_eq/lin | series1 | False | 5.761 | 3.030 | 6/6 | True |
| compare/filter_eq/lin | series2 | False | 4.735 | 2.352 | 6/6 | True |
| compare/filter_eq/sqlite | series1 | False | -0.432 | 1.083 | 3/6 | True |
| compare/filter_eq/sqlite | series2 | False | 1.330 | 1.501 | 6/6 | True |
| compare/filter_eq/duckdb | series1 | False | 2.312 | 7.152 | 3/6 | True |
| compare/filter_eq/duckdb | series2 | False | -3.402 | 5.370 | 2/6 | True |
| compare/filter_range/lin | series1 | False | 6.758 | 2.406 | 6/6 | True |
| compare/filter_range/lin | series2 | False | 4.939 | 2.132 | 5/6 | True |
| compare/filter_range/sqlite | series1 | False | -0.139 | 1.183 | 2/6 | True |
| compare/filter_range/sqlite | series2 | False | -0.100 | 1.068 | 2/6 | True |
| compare/filter_range/duckdb | series1 | False | -0.515 | 3.166 | 3/6 | True |
| compare/filter_range/duckdb | series2 | False | 1.397 | 2.770 | 4/6 | True |
| compare/agg_count_by/lin | series1 | False | -1.580 | 1.035 | 2/6 | True |
| compare/agg_count_by/lin | series2 | False | 1.284 | 2.599 | 5/6 | True |
| compare/agg_count_by/sqlite | series1 | False | -0.414 | 0.756 | 2/6 | True |
| compare/agg_count_by/sqlite | series2 | False | -0.037 | 0.752 | 3/6 | True |
| compare/agg_count_by/duckdb | series1 | False | -14.419 | 8.901 | 3/6 | False |
| compare/agg_count_by/duckdb | series2 | False | -14.991 | 3.870 | 0/6 | False |
| compare/wal_scalar_full_update/lin | series1 | True | 63.342 | 2.115 | 6/6 | True |
| compare/wal_scalar_full_update/lin | series2 | True | 63.167 | 2.209 | 6/6 | True |
| compare/wal_scalar_normal_update/lin | series1 | True | 97.529 | 4.358 | 6/6 | True |
| compare/wal_scalar_normal_update/lin | series2 | True | 97.673 | 1.566 | 6/6 | True |
| compare/tx_commit_1row_100k/lin | series1 | False | 55.112 | 0.990 | 6/6 | True |
| compare/tx_commit_1row_100k/lin | series2 | False | 56.502 | 3.202 | 6/6 | True |
| compare/tx_commit_1row_100k/sqlite | series1 | False | 8.485 | 12.249 | 4/6 | True |
| compare/tx_commit_1row_100k/sqlite | series2 | False | 13.329 | 10.306 | 5/6 | True |
| compare/tx_rollback_1row_100k/lin | series1 | False | 41.122 | 5.318 | 6/6 | True |
| compare/tx_rollback_1row_100k/lin | series2 | False | 42.211 | 4.825 | 6/6 | True |
| compare/tx_rollback_1row_100k/sqlite | series1 | False | 7.086 | 6.106 | 4/6 | True |
| compare/tx_rollback_1row_100k/sqlite | series2 | False | 3.623 | 5.267 | 5/6 | True |

| Lin case | Peer | Series | Lin/peer | Lin wins |
|---|---|---|---:|---:|
| compare/durable_update_1row_1k/lin | sqlite_delete | series1 | 0.4912 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series1 | 1.4092 | 0/6 |
| compare/durable_update_1row_1k/lin | sqlite_delete | series2 | 0.4636 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series2 | 1.3721 | 0/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series1 | 0.4809 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series1 | 1.3988 | 0/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series2 | 0.4842 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series2 | 1.3521 | 0/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series1 | 0.4243 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series1 | 1.2690 | 0/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series2 | 0.5396 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series2 | 1.4533 | 0/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series1 | 0.2546 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series1 | 1.5510 | 0/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series2 | 0.2666 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series2 | 1.6078 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series1 | 0.5180 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series1 | 1.6159 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series2 | 0.6123 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series2 | 1.5420 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series1 | 0.5914 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series1 | 1.7001 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series2 | 0.6334 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series2 | 1.7470 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series1 | 0.5767 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series1 | 1.6506 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series2 | 0.6381 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series2 | 1.6850 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series1 | 0.2622 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series1 | 1.2899 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series2 | 0.2756 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series2 | 1.3069 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series1 | 0.8877 | 4/6 |
| compare/update_1row_1k/lin | sqlite | series2 | 0.8666 | 6/6 |
| compare/delete_1row_1k/lin | sqlite | series1 | 2.3126 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series2 | 2.2314 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series1 | 0.8133 | 6/6 |
| compare/update_1row_10k/lin | sqlite | series2 | 0.8102 | 6/6 |
| compare/delete_1row_10k/lin | sqlite | series1 | 1.9225 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series2 | 1.9383 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series1 | 0.8388 | 5/6 |
| compare/update_1row_100k/lin | sqlite | series2 | 0.8659 | 6/6 |
| compare/delete_1row_100k/lin | sqlite | series1 | 1.9655 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series2 | 1.9745 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series1 | 0.7984 | 6/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series2 | 0.7398 | 6/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series1 | 1.1903 | 0/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series2 | 1.2128 | 1/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series1 | 0.6993 | 6/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series2 | 0.6804 | 6/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series1 | 1.0954 | 1/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series2 | 1.0727 | 1/6 |
| compare/insert_bulk_1k/lin | sqlite | series1 | 1.2973 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series2 | 1.2750 | 0/6 |
| compare/insert_bulk_10k/lin | sqlite | series1 | 1.0771 | 0/6 |
| compare/insert_bulk_10k/lin | sqlite | series2 | 1.0854 | 0/6 |
| compare/durable_insert_1k/lin | sqlite | series1 | 1.0306 | 1/6 |
| compare/durable_insert_1k/lin | sqlite | series2 | 1.0602 | 0/6 |
| compare/durable_insert_10k/lin | sqlite | series1 | 1.0388 | 0/6 |
| compare/durable_insert_10k/lin | sqlite | series2 | 1.0503 | 0/6 |
| compare/filter_eq/lin | sqlite | series1 | 0.0098 | 6/6 |
| compare/filter_eq/lin | duckdb | series1 | 0.0055 | 6/6 |
| compare/filter_eq/lin | sqlite | series2 | 0.0098 | 6/6 |
| compare/filter_eq/lin | duckdb | series2 | 0.0055 | 6/6 |
| compare/filter_range/lin | sqlite | series1 | 0.0142 | 6/6 |
| compare/filter_range/lin | duckdb | series1 | 0.0044 | 6/6 |
| compare/filter_range/lin | sqlite | series2 | 0.0142 | 6/6 |
| compare/filter_range/lin | duckdb | series2 | 0.0043 | 6/6 |
| compare/agg_count_by/lin | sqlite | series1 | 0.3704 | 6/6 |
| compare/agg_count_by/lin | duckdb | series1 | 0.2149 | 6/6 |
| compare/agg_count_by/lin | sqlite | series2 | 0.3602 | 6/6 |
| compare/agg_count_by/lin | duckdb | series2 | 0.2178 | 6/6 |
| compare/tx_commit_1row_100k/lin | sqlite | series1 | 0.6704 | 6/6 |
| compare/tx_commit_1row_100k/lin | sqlite | series2 | 0.7100 | 6/6 |
| compare/tx_rollback_1row_100k/lin | sqlite | series1 | 1.0839 | 2/6 |
| compare/tx_rollback_1row_100k/lin | sqlite | series2 | 1.0609 | 2/6 |

Source gate: False. Numeric peer wins: 245/456. Whole-process RSS in summary.json includes fixture, oracle and reopen; it is not isolated operation memory.
