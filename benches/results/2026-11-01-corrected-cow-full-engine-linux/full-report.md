# Corrected COW + CAS + UPDATE: full-engine paired gate

All 67 cases retained. Each series has six process pairs. Positive gain means candidate faster. Targets require 6/6 positive pairs and median gain above A/A. Controls require gain >= -max(2%, A/A). Full eight-peer objective remains false.

| Case | Series | Target | Gain % | A/A % | Positive | Pass |
|---|---|---|---:|---:|---:|---|
| compare/durable_update_1row_1k/lin | series1 | False | 0.636 | 5.029 | 3/6 | True |
| compare/durable_update_1row_1k/lin | series2 | False | -1.976 | 5.889 | 3/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series1 | False | -3.353 | 1.408 | 1/6 | False |
| compare/durable_update_1row_1k/sqlite_delete | series2 | False | -0.341 | 4.307 | 3/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series1 | False | -2.716 | 5.913 | 3/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series2 | False | -1.305 | 1.279 | 2/6 | True |
| compare/durable_delete_1row_1k/lin | series1 | False | 5.996 | 8.350 | 5/6 | True |
| compare/durable_delete_1row_1k/lin | series2 | False | 7.929 | 11.211 | 4/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series1 | False | 8.782 | 8.619 | 4/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series2 | False | -0.837 | 1.233 | 2/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series1 | False | -3.207 | 3.807 | 1/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series2 | False | -1.100 | 3.496 | 2/6 | True |
| compare/durable_commit_1row_1k/lin | series1 | False | 3.495 | 5.415 | 5/6 | True |
| compare/durable_commit_1row_1k/lin | series2 | False | 2.481 | 3.657 | 4/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series1 | False | 4.215 | 2.008 | 4/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series2 | False | 2.109 | 8.824 | 4/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series1 | False | -2.309 | 3.035 | 1/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series2 | False | -4.184 | 4.660 | 1/6 | True |
| compare/durable_rollback_1row_1k/lin | series1 | False | 21.419 | 5.550 | 6/6 | True |
| compare/durable_rollback_1row_1k/lin | series2 | False | 19.610 | 8.090 | 6/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series1 | False | 0.844 | 1.141 | 4/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series2 | False | -1.701 | 1.432 | 2/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series1 | False | 4.290 | 0.983 | 5/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series2 | False | 0.130 | 3.428 | 4/6 | True |
| compare/durable_update_1row_10k/lin | series1 | False | 0.959 | 6.022 | 3/6 | True |
| compare/durable_update_1row_10k/lin | series2 | False | 3.667 | 2.785 | 4/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series1 | False | -0.129 | 6.375 | 3/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series2 | False | -0.720 | 4.828 | 3/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series1 | False | -1.911 | 4.560 | 2/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series2 | False | 0.650 | 4.797 | 3/6 | True |
| compare/durable_delete_1row_10k/lin | series1 | False | -0.534 | 4.347 | 3/6 | True |
| compare/durable_delete_1row_10k/lin | series2 | False | 1.913 | 3.533 | 4/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series1 | False | -0.360 | 5.165 | 3/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series2 | False | 0.044 | 3.958 | 3/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series1 | False | 2.773 | 3.833 | 5/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series2 | False | 7.032 | 8.246 | 6/6 | True |
| compare/durable_commit_1row_10k/lin | series1 | False | -4.498 | 7.699 | 2/6 | True |
| compare/durable_commit_1row_10k/lin | series2 | False | 2.989 | 5.851 | 3/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series1 | False | 0.767 | 4.007 | 3/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series2 | False | 2.755 | 7.613 | 3/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series1 | False | -0.372 | 5.634 | 3/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series2 | False | 4.798 | 1.990 | 4/6 | True |
| compare/durable_rollback_1row_10k/lin | series1 | False | 14.166 | 2.185 | 6/6 | True |
| compare/durable_rollback_1row_10k/lin | series2 | False | 15.631 | 2.772 | 6/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series1 | False | 2.566 | 3.732 | 6/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series2 | False | -2.659 | 5.086 | 2/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series1 | False | 2.122 | 5.399 | 4/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series2 | False | -0.263 | 3.067 | 3/6 | True |
| compare/update_1row_1k/lin | series1 | True | 51.106 | 6.070 | 6/6 | True |
| compare/update_1row_1k/lin | series2 | True | 54.836 | 2.470 | 6/6 | True |
| compare/update_1row_1k/sqlite | series1 | False | -1.753 | 9.315 | 2/6 | True |
| compare/update_1row_1k/sqlite | series2 | False | 2.125 | 0.667 | 4/6 | True |
| compare/delete_1row_1k/lin | series1 | False | 0.973 | 3.680 | 3/6 | True |
| compare/delete_1row_1k/lin | series2 | False | -1.293 | 2.598 | 2/6 | True |
| compare/delete_1row_1k/sqlite | series1 | False | -2.709 | 1.287 | 2/6 | False |
| compare/delete_1row_1k/sqlite | series2 | False | 1.149 | 1.683 | 4/6 | True |
| compare/update_1row_10k/lin | series1 | True | 56.538 | 2.819 | 6/6 | True |
| compare/update_1row_10k/lin | series2 | True | 54.149 | 2.758 | 6/6 | True |
| compare/update_1row_10k/sqlite | series1 | False | -2.412 | 3.392 | 2/6 | True |
| compare/update_1row_10k/sqlite | series2 | False | 7.103 | 5.800 | 5/6 | True |
| compare/delete_1row_10k/lin | series1 | False | 2.663 | 1.889 | 5/6 | True |
| compare/delete_1row_10k/lin | series2 | False | 2.745 | 2.330 | 6/6 | True |
| compare/delete_1row_10k/sqlite | series1 | False | -1.747 | 3.658 | 1/6 | True |
| compare/delete_1row_10k/sqlite | series2 | False | -4.761 | 2.727 | 3/6 | False |
| compare/update_1row_100k/lin | series1 | True | 56.149 | 2.019 | 6/6 | True |
| compare/update_1row_100k/lin | series2 | True | 54.584 | 3.551 | 6/6 | True |
| compare/update_1row_100k/sqlite | series1 | False | 1.083 | 5.176 | 3/6 | True |
| compare/update_1row_100k/sqlite | series2 | False | 18.446 | 8.000 | 6/6 | True |
| compare/delete_1row_100k/lin | series1 | False | 4.918 | 3.466 | 5/6 | True |
| compare/delete_1row_100k/lin | series2 | False | 4.207 | 4.407 | 6/6 | True |
| compare/delete_1row_100k/sqlite | series1 | False | 14.998 | 5.735 | 6/6 | True |
| compare/delete_1row_100k/sqlite | series2 | False | 13.150 | 2.294 | 5/6 | True |
| compare/tx_commit_1row_1k/lin | series1 | False | 50.365 | 1.785 | 6/6 | True |
| compare/tx_commit_1row_1k/lin | series2 | False | 56.151 | 1.920 | 6/6 | True |
| compare/tx_commit_1row_1k/sqlite | series1 | False | 5.380 | 7.108 | 6/6 | True |
| compare/tx_commit_1row_1k/sqlite | series2 | False | -0.190 | 1.233 | 3/6 | True |
| compare/tx_rollback_1row_1k/lin | series1 | False | 39.284 | 4.206 | 6/6 | True |
| compare/tx_rollback_1row_1k/lin | series2 | False | 41.652 | 4.103 | 6/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series1 | False | -0.160 | 1.209 | 3/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series2 | False | 2.114 | 1.998 | 3/6 | True |
| compare/tx_commit_1row_10k/lin | series1 | False | 54.551 | 5.205 | 6/6 | True |
| compare/tx_commit_1row_10k/lin | series2 | False | 55.225 | 2.123 | 6/6 | True |
| compare/tx_commit_1row_10k/sqlite | series1 | False | -6.773 | 3.261 | 1/6 | False |
| compare/tx_commit_1row_10k/sqlite | series2 | False | 3.469 | 4.354 | 4/6 | True |
| compare/tx_rollback_1row_10k/lin | series1 | False | 43.163 | 3.391 | 6/6 | True |
| compare/tx_rollback_1row_10k/lin | series2 | False | 40.673 | 3.287 | 6/6 | True |
| compare/tx_rollback_1row_10k/sqlite | series1 | False | -5.636 | 4.339 | 1/6 | False |
| compare/tx_rollback_1row_10k/sqlite | series2 | False | 0.406 | 4.228 | 3/6 | True |
| compare/insert_bulk_1k/lin | series1 | False | -1.694 | 2.291 | 1/6 | True |
| compare/insert_bulk_1k/lin | series2 | False | 0.759 | 1.375 | 4/6 | True |
| compare/insert_bulk_1k/sqlite | series1 | False | 0.210 | 1.927 | 4/6 | True |
| compare/insert_bulk_1k/sqlite | series2 | False | -0.101 | 0.701 | 3/6 | True |
| compare/insert_bulk_10k/lin | series1 | False | 45.397 | 1.049 | 6/6 | True |
| compare/insert_bulk_10k/lin | series2 | False | 45.027 | 0.894 | 6/6 | True |
| compare/insert_bulk_10k/sqlite | series1 | False | 0.155 | 0.641 | 3/6 | True |
| compare/insert_bulk_10k/sqlite | series2 | False | -0.185 | 1.400 | 3/6 | True |
| compare/durable_insert_1k/lin | series1 | False | 1.811 | 3.490 | 4/6 | True |
| compare/durable_insert_1k/lin | series2 | False | 1.855 | 3.363 | 5/6 | True |
| compare/durable_insert_1k/sqlite | series1 | False | -0.237 | 0.566 | 3/6 | True |
| compare/durable_insert_1k/sqlite | series2 | False | 0.106 | 1.611 | 3/6 | True |
| compare/durable_insert_10k/lin | series1 | False | 37.462 | 19.930 | 5/6 | True |
| compare/durable_insert_10k/lin | series2 | False | 19.211 | 0.863 | 4/6 | True |
| compare/durable_insert_10k/sqlite | series1 | False | -0.470 | 1.036 | 1/6 | True |
| compare/durable_insert_10k/sqlite | series2 | False | 0.321 | 0.375 | 4/6 | True |
| compare/filter_eq/lin | series1 | False | 6.899 | 2.871 | 6/6 | True |
| compare/filter_eq/lin | series2 | False | 8.220 | 1.573 | 6/6 | True |
| compare/filter_eq/sqlite | series1 | False | -0.359 | 1.361 | 2/6 | True |
| compare/filter_eq/sqlite | series2 | False | -0.596 | 2.505 | 2/6 | True |
| compare/filter_eq/duckdb | series1 | False | 2.805 | 6.245 | 4/6 | True |
| compare/filter_eq/duckdb | series2 | False | -1.329 | 3.289 | 3/6 | True |
| compare/filter_range/lin | series1 | False | 3.344 | 3.131 | 6/6 | True |
| compare/filter_range/lin | series2 | False | 6.114 | 0.729 | 6/6 | True |
| compare/filter_range/sqlite | series1 | False | -0.738 | 0.379 | 2/6 | True |
| compare/filter_range/sqlite | series2 | False | 0.250 | 0.722 | 3/6 | True |
| compare/filter_range/duckdb | series1 | False | 0.437 | 0.954 | 3/6 | True |
| compare/filter_range/duckdb | series2 | False | -4.496 | 3.299 | 2/6 | False |
| compare/agg_count_by/lin | series1 | False | 3.063 | 1.335 | 5/6 | True |
| compare/agg_count_by/lin | series2 | False | 0.236 | 1.612 | 4/6 | True |
| compare/agg_count_by/sqlite | series1 | False | 0.271 | 1.195 | 3/6 | True |
| compare/agg_count_by/sqlite | series2 | False | -1.338 | 0.350 | 2/6 | True |
| compare/agg_count_by/duckdb | series1 | False | -19.420 | 13.632 | 2/6 | False |
| compare/agg_count_by/duckdb | series2 | False | -38.902 | 2.816 | 0/6 | False |
| compare/wal_scalar_full_update/lin | series1 | True | 63.096 | 3.701 | 6/6 | True |
| compare/wal_scalar_full_update/lin | series2 | True | 63.684 | 5.565 | 6/6 | True |
| compare/wal_scalar_normal_update/lin | series1 | True | 97.623 | 2.047 | 6/6 | True |
| compare/wal_scalar_normal_update/lin | series2 | True | 97.757 | 1.108 | 6/6 | True |
| compare/tx_commit_1row_100k/lin | series1 | False | 56.359 | 3.529 | 6/6 | True |
| compare/tx_commit_1row_100k/lin | series2 | False | 54.827 | 2.998 | 6/6 | True |
| compare/tx_commit_1row_100k/sqlite | series1 | False | 1.842 | 6.315 | 4/6 | True |
| compare/tx_commit_1row_100k/sqlite | series2 | False | 3.602 | 6.758 | 3/6 | True |
| compare/tx_rollback_1row_100k/lin | series1 | False | 43.646 | 1.765 | 6/6 | True |
| compare/tx_rollback_1row_100k/lin | series2 | False | 41.848 | 3.907 | 6/6 | True |
| compare/tx_rollback_1row_100k/sqlite | series1 | False | 6.688 | 10.093 | 5/6 | True |
| compare/tx_rollback_1row_100k/sqlite | series2 | False | 1.999 | 5.042 | 3/6 | True |

| Lin case | Peer | Series | Lin/peer | Lin wins |
|---|---|---|---:|---:|
| compare/durable_update_1row_1k/lin | sqlite_delete | series1 | 0.5205 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series1 | 1.4119 | 0/6 |
| compare/durable_update_1row_1k/lin | sqlite_delete | series2 | 0.5375 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series2 | 1.3904 | 0/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series1 | 0.4484 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series1 | 1.2831 | 0/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series2 | 0.4898 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series2 | 1.3180 | 0/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series1 | 0.4603 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series1 | 1.2033 | 0/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series2 | 0.5033 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series2 | 1.2827 | 0/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series1 | 0.1938 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series1 | 1.2057 | 0/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series2 | 0.2257 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series2 | 1.3404 | 1/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series1 | 0.6352 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series1 | 1.5651 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series2 | 0.6443 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series2 | 1.5997 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series1 | 0.6041 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series1 | 1.6298 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series2 | 0.6064 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series2 | 1.6625 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series1 | 0.6250 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series1 | 1.6651 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series2 | 0.6505 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series2 | 1.6466 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series1 | 0.2532 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series1 | 1.2806 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series2 | 0.2540 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series2 | 1.4122 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series1 | 0.9190 | 5/6 |
| compare/update_1row_1k/lin | sqlite | series2 | 0.8156 | 6/6 |
| compare/delete_1row_1k/lin | sqlite | series1 | 2.1369 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series2 | 2.1887 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series1 | 0.7413 | 6/6 |
| compare/update_1row_10k/lin | sqlite | series2 | 0.9326 | 3/6 |
| compare/delete_1row_10k/lin | sqlite | series1 | 1.9626 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series2 | 1.9305 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series1 | 0.7205 | 6/6 |
| compare/update_1row_100k/lin | sqlite | series2 | 0.8137 | 4/6 |
| compare/delete_1row_100k/lin | sqlite | series1 | 2.0511 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series2 | 2.0229 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series1 | 0.7576 | 6/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series2 | 0.6298 | 6/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series1 | 1.2410 | 1/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series2 | 1.1008 | 1/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series1 | 0.6452 | 6/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series2 | 0.6821 | 6/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series1 | 1.0082 | 3/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series2 | 1.1726 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series1 | 1.2473 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series2 | 1.1960 | 0/6 |
| compare/insert_bulk_10k/lin | sqlite | series1 | 0.9889 | 5/6 |
| compare/insert_bulk_10k/lin | sqlite | series2 | 1.0036 | 3/6 |
| compare/durable_insert_1k/lin | sqlite | series1 | 0.9088 | 6/6 |
| compare/durable_insert_1k/lin | sqlite | series2 | 0.9116 | 6/6 |
| compare/durable_insert_10k/lin | sqlite | series1 | 0.9748 | 6/6 |
| compare/durable_insert_10k/lin | sqlite | series2 | 0.9584 | 6/6 |
| compare/filter_eq/lin | sqlite | series1 | 0.0098 | 6/6 |
| compare/filter_eq/lin | duckdb | series1 | 0.0052 | 6/6 |
| compare/filter_eq/lin | sqlite | series2 | 0.0097 | 6/6 |
| compare/filter_eq/lin | duckdb | series2 | 0.0053 | 6/6 |
| compare/filter_range/lin | sqlite | series1 | 0.0142 | 6/6 |
| compare/filter_range/lin | duckdb | series1 | 0.0045 | 6/6 |
| compare/filter_range/lin | sqlite | series2 | 0.0142 | 6/6 |
| compare/filter_range/lin | duckdb | series2 | 0.0044 | 6/6 |
| compare/agg_count_by/lin | sqlite | series1 | 0.3620 | 6/6 |
| compare/agg_count_by/lin | duckdb | series1 | 0.1867 | 6/6 |
| compare/agg_count_by/lin | sqlite | series2 | 0.3596 | 6/6 |
| compare/agg_count_by/lin | duckdb | series2 | 0.1818 | 6/6 |
| compare/tx_commit_1row_100k/lin | sqlite | series1 | 0.6755 | 6/6 |
| compare/tx_commit_1row_100k/lin | sqlite | series2 | 0.6224 | 6/6 |
| compare/tx_rollback_1row_100k/lin | sqlite | series1 | 1.0244 | 3/6 |
| compare/tx_rollback_1row_100k/lin | sqlite | series2 | 0.9028 | 4/6 |

Source gate: False. Numeric peer wins: 279/456. Whole-process RSS in summary.json includes fixture, oracle and reopen; it is not isolated operation memory.
