# Corrected COW with invariant external peer executable: full-engine gate

All67 cases retained. External peers use the identical baseline-built executable for every role; Lin uses its source role. Prior linked-peer failures are not reclassified. Each series has six process pairs. Positive gain means candidate faster. Targets require 6/6 positive pairs and median gain above A/A. Controls require gain >= -max(2%, A/A). Full eight-peer objective remains false.

| Case | Series | Target | Gain % | A/A % | Positive | Pass |
|---|---|---|---:|---:|---:|---|
| compare/durable_update_1row_1k/lin | series1 | False | 36.899 | 3.429 | 6/6 | True |
| compare/durable_update_1row_1k/lin | series2 | False | 38.566 | 5.973 | 6/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series1 | False | 5.695 | 7.239 | 6/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series2 | False | 3.899 | 2.118 | 5/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series1 | False | -2.285 | 3.955 | 2/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series2 | False | 5.878 | 6.462 | 4/6 | True |
| compare/durable_delete_1row_1k/lin | series1 | False | 36.100 | 7.055 | 6/6 | True |
| compare/durable_delete_1row_1k/lin | series2 | False | 38.992 | 4.982 | 6/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series1 | False | 2.698 | 7.326 | 5/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series2 | False | 0.161 | 3.621 | 3/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series1 | False | 0.007 | 8.288 | 3/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series2 | False | 1.405 | 2.453 | 4/6 | True |
| compare/durable_commit_1row_1k/lin | series1 | False | 34.424 | 7.486 | 6/6 | True |
| compare/durable_commit_1row_1k/lin | series2 | False | 38.805 | 7.057 | 6/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series1 | False | 2.371 | 4.893 | 5/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series2 | False | 4.370 | 7.181 | 6/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series1 | False | -6.431 | 4.990 | 1/6 | False |
| compare/durable_commit_1row_1k/sqlite_wal | series2 | False | -4.298 | 8.558 | 2/6 | True |
| compare/durable_rollback_1row_1k/lin | series1 | False | 19.089 | 3.100 | 6/6 | True |
| compare/durable_rollback_1row_1k/lin | series2 | False | 15.868 | 9.174 | 6/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series1 | False | -0.019 | 0.839 | 3/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series2 | False | -1.535 | 1.454 | 1/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series1 | False | -0.622 | 1.412 | 2/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series2 | False | -0.206 | 0.973 | 3/6 | True |
| compare/durable_update_1row_10k/lin | series1 | False | 43.100 | 6.580 | 6/6 | True |
| compare/durable_update_1row_10k/lin | series2 | False | 45.477 | 1.678 | 6/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series1 | False | -4.479 | 6.601 | 1/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series2 | False | 3.785 | 3.080 | 4/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series1 | False | 3.641 | 4.410 | 4/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series2 | False | -6.479 | 3.423 | 2/6 | False |
| compare/durable_delete_1row_10k/lin | series1 | False | 44.443 | 2.947 | 6/6 | True |
| compare/durable_delete_1row_10k/lin | series2 | False | 41.269 | 4.847 | 6/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series1 | False | 0.306 | 10.256 | 3/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series2 | False | -0.452 | 5.779 | 3/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series1 | False | -0.945 | 6.919 | 3/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series2 | False | -0.552 | 7.135 | 3/6 | True |
| compare/durable_commit_1row_10k/lin | series1 | False | 42.073 | 10.290 | 6/6 | True |
| compare/durable_commit_1row_10k/lin | series2 | False | 46.000 | 3.601 | 6/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series1 | False | -5.875 | 5.728 | 0/6 | False |
| compare/durable_commit_1row_10k/sqlite_delete | series2 | False | 1.449 | 4.283 | 4/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series1 | False | 0.295 | 5.152 | 3/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series2 | False | -1.296 | 3.794 | 3/6 | True |
| compare/durable_rollback_1row_10k/lin | series1 | False | 15.178 | 1.888 | 6/6 | True |
| compare/durable_rollback_1row_10k/lin | series2 | False | 15.893 | 2.452 | 6/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series1 | False | -0.451 | 1.689 | 3/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series2 | False | 0.558 | 1.422 | 4/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series1 | False | -0.244 | 2.514 | 3/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series2 | False | 2.633 | 3.321 | 4/6 | True |
| compare/update_1row_1k/lin | series1 | True | 58.509 | 1.002 | 6/6 | True |
| compare/update_1row_1k/lin | series2 | True | 58.595 | 1.461 | 6/6 | True |
| compare/update_1row_1k/sqlite | series1 | False | -2.803 | 1.711 | 1/6 | False |
| compare/update_1row_1k/sqlite | series2 | False | -3.432 | 3.853 | 2/6 | True |
| compare/delete_1row_1k/lin | series1 | False | -1.356 | 3.110 | 2/6 | True |
| compare/delete_1row_1k/lin | series2 | False | 1.110 | 4.862 | 5/6 | True |
| compare/delete_1row_1k/sqlite | series1 | False | -0.665 | 3.894 | 2/6 | True |
| compare/delete_1row_1k/sqlite | series2 | False | 3.401 | 4.509 | 4/6 | True |
| compare/update_1row_10k/lin | series1 | True | 55.778 | 2.357 | 6/6 | True |
| compare/update_1row_10k/lin | series2 | True | 55.635 | 1.374 | 6/6 | True |
| compare/update_1row_10k/sqlite | series1 | False | -3.126 | 3.674 | 2/6 | True |
| compare/update_1row_10k/sqlite | series2 | False | -3.694 | 6.071 | 3/6 | True |
| compare/delete_1row_10k/lin | series1 | False | -0.154 | 2.652 | 3/6 | True |
| compare/delete_1row_10k/lin | series2 | False | 1.541 | 1.939 | 4/6 | True |
| compare/delete_1row_10k/sqlite | series1 | False | 0.926 | 4.431 | 3/6 | True |
| compare/delete_1row_10k/sqlite | series2 | False | 1.036 | 5.397 | 4/6 | True |
| compare/update_1row_100k/lin | series1 | True | 57.747 | 7.648 | 6/6 | True |
| compare/update_1row_100k/lin | series2 | True | 59.055 | 4.328 | 6/6 | True |
| compare/update_1row_100k/sqlite | series1 | False | 6.994 | 17.328 | 4/6 | True |
| compare/update_1row_100k/sqlite | series2 | False | 3.357 | 11.113 | 4/6 | True |
| compare/delete_1row_100k/lin | series1 | False | 5.113 | 3.714 | 5/6 | True |
| compare/delete_1row_100k/lin | series2 | False | 8.129 | 4.960 | 4/6 | True |
| compare/delete_1row_100k/sqlite | series1 | False | 12.442 | 9.554 | 5/6 | True |
| compare/delete_1row_100k/sqlite | series2 | False | -13.260 | 19.990 | 2/6 | True |
| compare/tx_commit_1row_1k/lin | series1 | False | 55.719 | 3.680 | 6/6 | True |
| compare/tx_commit_1row_1k/lin | series2 | False | 58.459 | 1.595 | 6/6 | True |
| compare/tx_commit_1row_1k/sqlite | series1 | False | -0.500 | 2.523 | 3/6 | True |
| compare/tx_commit_1row_1k/sqlite | series2 | False | -1.580 | 2.305 | 2/6 | True |
| compare/tx_rollback_1row_1k/lin | series1 | False | 46.152 | 2.466 | 6/6 | True |
| compare/tx_rollback_1row_1k/lin | series2 | False | 44.361 | 3.568 | 6/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series1 | False | 2.273 | 2.060 | 6/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series2 | False | 1.569 | 3.848 | 5/6 | True |
| compare/tx_commit_1row_10k/lin | series1 | False | 52.839 | 1.998 | 6/6 | True |
| compare/tx_commit_1row_10k/lin | series2 | False | 52.402 | 3.264 | 6/6 | True |
| compare/tx_commit_1row_10k/sqlite | series1 | False | -0.847 | 3.452 | 3/6 | True |
| compare/tx_commit_1row_10k/sqlite | series2 | False | 2.825 | 8.401 | 3/6 | True |
| compare/tx_rollback_1row_10k/lin | series1 | False | 41.502 | 2.103 | 6/6 | True |
| compare/tx_rollback_1row_10k/lin | series2 | False | 41.329 | 3.606 | 6/6 | True |
| compare/tx_rollback_1row_10k/sqlite | series1 | False | -2.111 | 1.641 | 2/6 | False |
| compare/tx_rollback_1row_10k/sqlite | series2 | False | -0.665 | 3.675 | 3/6 | True |
| compare/insert_bulk_1k/lin | series1 | False | 4.332 | 1.184 | 6/6 | True |
| compare/insert_bulk_1k/lin | series2 | False | 5.299 | 2.102 | 6/6 | True |
| compare/insert_bulk_1k/sqlite | series1 | False | -0.952 | 3.851 | 2/6 | True |
| compare/insert_bulk_1k/sqlite | series2 | False | 1.171 | 0.953 | 5/6 | True |
| compare/insert_bulk_10k/lin | series1 | False | 48.641 | 0.943 | 6/6 | True |
| compare/insert_bulk_10k/lin | series2 | False | 48.316 | 1.074 | 6/6 | True |
| compare/insert_bulk_10k/sqlite | series1 | False | 0.169 | 0.346 | 4/6 | True |
| compare/insert_bulk_10k/sqlite | series2 | False | 0.041 | 0.650 | 4/6 | True |
| compare/durable_insert_1k/lin | series1 | False | 6.611 | 5.154 | 5/6 | True |
| compare/durable_insert_1k/lin | series2 | False | 3.312 | 2.442 | 5/6 | True |
| compare/durable_insert_1k/sqlite | series1 | False | 0.420 | 1.047 | 3/6 | True |
| compare/durable_insert_1k/sqlite | series2 | False | -1.035 | 0.763 | 2/6 | True |
| compare/durable_insert_10k/lin | series1 | False | 43.428 | 1.719 | 6/6 | True |
| compare/durable_insert_10k/lin | series2 | False | 42.322 | 1.546 | 6/6 | True |
| compare/durable_insert_10k/sqlite | series1 | False | 0.282 | 1.499 | 3/6 | True |
| compare/durable_insert_10k/sqlite | series2 | False | 0.444 | 1.069 | 5/6 | True |
| compare/filter_eq/lin | series1 | False | 3.501 | 1.740 | 6/6 | True |
| compare/filter_eq/lin | series2 | False | 3.976 | 0.819 | 6/6 | True |
| compare/filter_eq/sqlite | series1 | False | -0.613 | 2.327 | 1/6 | True |
| compare/filter_eq/sqlite | series2 | False | -1.221 | 1.300 | 1/6 | True |
| compare/filter_eq/duckdb | series1 | False | -0.177 | 4.303 | 3/6 | True |
| compare/filter_eq/duckdb | series2 | False | 0.404 | 4.197 | 3/6 | True |
| compare/filter_range/lin | series1 | False | 4.617 | 1.298 | 6/6 | True |
| compare/filter_range/lin | series2 | False | 4.728 | 1.511 | 6/6 | True |
| compare/filter_range/sqlite | series1 | False | -0.207 | 0.654 | 3/6 | True |
| compare/filter_range/sqlite | series2 | False | 3.322 | 1.934 | 4/6 | True |
| compare/filter_range/duckdb | series1 | False | -1.319 | 1.566 | 2/6 | True |
| compare/filter_range/duckdb | series2 | False | -5.490 | 6.680 | 0/6 | True |
| compare/agg_count_by/lin | series1 | False | 8.697 | 0.298 | 5/6 | True |
| compare/agg_count_by/lin | series2 | False | 8.420 | 1.343 | 6/6 | True |
| compare/agg_count_by/sqlite | series1 | False | -0.309 | 0.580 | 2/6 | True |
| compare/agg_count_by/sqlite | series2 | False | 0.133 | 0.485 | 5/6 | True |
| compare/agg_count_by/duckdb | series1 | False | 3.356 | 3.216 | 4/6 | True |
| compare/agg_count_by/duckdb | series2 | False | 0.484 | 3.675 | 4/6 | True |
| compare/wal_scalar_full_update/lin | series1 | True | 78.400 | 2.437 | 6/6 | True |
| compare/wal_scalar_full_update/lin | series2 | True | 80.039 | 3.207 | 6/6 | True |
| compare/wal_scalar_normal_update/lin | series1 | True | 97.750 | 2.186 | 6/6 | True |
| compare/wal_scalar_normal_update/lin | series2 | True | 97.802 | 0.618 | 6/6 | True |
| compare/tx_commit_1row_100k/lin | series1 | False | 55.519 | 2.694 | 6/6 | True |
| compare/tx_commit_1row_100k/lin | series2 | False | 54.274 | 1.324 | 6/6 | True |
| compare/tx_commit_1row_100k/sqlite | series1 | False | -19.729 | 19.528 | 1/6 | False |
| compare/tx_commit_1row_100k/sqlite | series2 | False | 9.058 | 22.570 | 4/6 | True |
| compare/tx_rollback_1row_100k/lin | series1 | False | 47.752 | 5.345 | 6/6 | True |
| compare/tx_rollback_1row_100k/lin | series2 | False | 45.289 | 4.623 | 6/6 | True |
| compare/tx_rollback_1row_100k/sqlite | series1 | False | -11.469 | 22.218 | 1/6 | True |
| compare/tx_rollback_1row_100k/sqlite | series2 | False | 2.139 | 10.304 | 4/6 | True |

| Lin case | Peer | Series | Lin/peer | Lin wins |
|---|---|---|---:|---:|
| compare/durable_update_1row_1k/lin | sqlite_delete | series1 | 0.2914 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series1 | 0.7831 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_delete | series2 | 0.2724 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series2 | 0.7832 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series1 | 0.2897 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series1 | 0.7956 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series2 | 0.2747 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series2 | 0.8012 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series1 | 0.2832 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series1 | 0.7307 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series2 | 0.2740 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series2 | 0.7793 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series1 | 0.1927 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series1 | 1.1690 | 0/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series2 | 0.2042 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series2 | 1.2527 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series1 | 0.3501 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series1 | 0.9763 | 4/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series2 | 0.3437 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series2 | 0.9004 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series1 | 0.3585 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series1 | 0.9387 | 5/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series2 | 0.3505 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series2 | 0.9588 | 4/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series1 | 0.3369 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series1 | 0.9623 | 4/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series2 | 0.3544 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series2 | 0.9588 | 4/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series1 | 0.2763 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series1 | 1.5309 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series2 | 0.2757 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series2 | 1.6081 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series1 | 0.7306 | 6/6 |
| compare/update_1row_1k/lin | sqlite | series2 | 0.7458 | 6/6 |
| compare/delete_1row_1k/lin | sqlite | series1 | 2.2118 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series2 | 2.0806 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series1 | 0.8005 | 4/6 |
| compare/update_1row_10k/lin | sqlite | series2 | 0.8186 | 5/6 |
| compare/delete_1row_10k/lin | sqlite | series1 | 2.7430 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series2 | 2.5689 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series1 | 0.8030 | 5/6 |
| compare/update_1row_100k/lin | sqlite | series2 | 0.8617 | 4/6 |
| compare/delete_1row_100k/lin | sqlite | series1 | 2.1385 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series2 | 1.9679 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series1 | 0.6335 | 6/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series2 | 0.6030 | 6/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series1 | 1.0817 | 0/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series2 | 1.0744 | 1/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series1 | 0.7396 | 6/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series2 | 0.7774 | 6/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series1 | 1.1957 | 0/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series2 | 1.3285 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series1 | 1.1690 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series2 | 1.1546 | 0/6 |
| compare/insert_bulk_10k/lin | sqlite | series1 | 0.9374 | 6/6 |
| compare/insert_bulk_10k/lin | sqlite | series2 | 0.9263 | 5/6 |
| compare/durable_insert_1k/lin | sqlite | series1 | 0.8670 | 6/6 |
| compare/durable_insert_1k/lin | sqlite | series2 | 0.8515 | 6/6 |
| compare/durable_insert_10k/lin | sqlite | series1 | 0.9025 | 6/6 |
| compare/durable_insert_10k/lin | sqlite | series2 | 0.8987 | 6/6 |
| compare/filter_eq/lin | sqlite | series1 | 0.0097 | 6/6 |
| compare/filter_eq/lin | duckdb | series1 | 0.0053 | 6/6 |
| compare/filter_eq/lin | sqlite | series2 | 0.0096 | 6/6 |
| compare/filter_eq/lin | duckdb | series2 | 0.0055 | 6/6 |
| compare/filter_range/lin | sqlite | series1 | 0.0142 | 6/6 |
| compare/filter_range/lin | duckdb | series1 | 0.0045 | 6/6 |
| compare/filter_range/lin | sqlite | series2 | 0.0143 | 6/6 |
| compare/filter_range/lin | duckdb | series2 | 0.0044 | 6/6 |
| compare/agg_count_by/lin | sqlite | series1 | 0.3365 | 6/6 |
| compare/agg_count_by/lin | duckdb | series1 | 0.2336 | 6/6 |
| compare/agg_count_by/lin | sqlite | series2 | 0.3341 | 6/6 |
| compare/agg_count_by/lin | duckdb | series2 | 0.2331 | 6/6 |
| compare/tx_commit_1row_100k/lin | sqlite | series1 | 0.7393 | 6/6 |
| compare/tx_commit_1row_100k/lin | sqlite | series2 | 0.7232 | 6/6 |
| compare/tx_rollback_1row_100k/lin | sqlite | series1 | 0.9346 | 5/6 |
| compare/tx_rollback_1row_100k/lin | sqlite | series2 | 1.0608 | 1/6 |

Source gate: False. Numeric peer wins: 339/456. Whole-process RSS in summary.json includes fixture, oracle and reopen; it is not isolated operation memory.
