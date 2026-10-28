# COW and single-statement CAS: full-engine paired gate

All 67 cases retained. Each series has six process pairs. Positive gain means candidate faster. Targets require 6/6 positive pairs and median gain above A/A. Controls require gain >= -max(2%, A/A). Full eight-peer objective remains false.

| Case | Series | Target | Gain % | A/A % | Positive | Pass |
|---|---|---|---:|---:|---:|---|
| compare/durable_update_1row_1k/lin | series1 | False | 5.972 | 7.908 | 4/6 | True |
| compare/durable_update_1row_1k/lin | series2 | False | -2.877 | 1.529 | 3/6 | False |
| compare/durable_update_1row_1k/sqlite_delete | series1 | False | 0.514 | 10.355 | 3/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series2 | False | 0.815 | 7.625 | 3/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series1 | False | 5.123 | 4.897 | 4/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series2 | False | -3.250 | 4.415 | 1/6 | True |
| compare/durable_delete_1row_1k/lin | series1 | False | 1.323 | 4.716 | 3/6 | True |
| compare/durable_delete_1row_1k/lin | series2 | False | 2.049 | 21.638 | 3/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series1 | False | -1.026 | 4.591 | 1/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series2 | False | -0.397 | 5.547 | 3/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series1 | False | 1.760 | 4.806 | 4/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series2 | False | 4.899 | 7.708 | 5/6 | True |
| compare/durable_commit_1row_1k/lin | series1 | False | 0.682 | 9.863 | 3/6 | True |
| compare/durable_commit_1row_1k/lin | series2 | False | 1.042 | 3.346 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series1 | False | -1.908 | 2.446 | 1/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series2 | False | 1.795 | 1.925 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series1 | False | 1.163 | 2.803 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series2 | False | 3.206 | 11.053 | 4/6 | True |
| compare/durable_rollback_1row_1k/lin | series1 | False | 17.355 | 4.822 | 5/6 | True |
| compare/durable_rollback_1row_1k/lin | series2 | False | 11.768 | 7.008 | 6/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series1 | False | 0.662 | 1.081 | 3/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series2 | False | 0.499 | 2.755 | 4/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series1 | False | 3.078 | 2.376 | 4/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series2 | False | 1.981 | 1.560 | 5/6 | True |
| compare/durable_update_1row_10k/lin | series1 | False | 3.383 | 7.928 | 5/6 | True |
| compare/durable_update_1row_10k/lin | series2 | False | 1.835 | 2.813 | 5/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series1 | False | -1.707 | 11.818 | 3/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series2 | False | -2.974 | 5.284 | 2/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series1 | False | 1.564 | 7.160 | 4/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series2 | False | 2.061 | 9.420 | 4/6 | True |
| compare/durable_delete_1row_10k/lin | series1 | False | 3.691 | 3.392 | 4/6 | True |
| compare/durable_delete_1row_10k/lin | series2 | False | -0.844 | 4.950 | 3/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series1 | False | 1.320 | 3.408 | 6/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series2 | False | -4.988 | 7.871 | 1/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series1 | False | 3.476 | 9.078 | 5/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series2 | False | 1.131 | 6.501 | 3/6 | True |
| compare/durable_commit_1row_10k/lin | series1 | False | 2.419 | 3.481 | 4/6 | True |
| compare/durable_commit_1row_10k/lin | series2 | False | 1.808 | 3.383 | 4/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series1 | False | 7.411 | 9.785 | 4/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series2 | False | -1.405 | 2.419 | 2/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series1 | False | -2.668 | 1.559 | 2/6 | False |
| compare/durable_commit_1row_10k/sqlite_wal | series2 | False | -0.414 | 4.555 | 2/6 | True |
| compare/durable_rollback_1row_10k/lin | series1 | False | 12.430 | 1.523 | 6/6 | True |
| compare/durable_rollback_1row_10k/lin | series2 | False | 10.993 | 1.382 | 6/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series1 | False | 1.978 | 2.320 | 4/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series2 | False | 0.742 | 8.284 | 5/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series1 | False | -2.746 | 2.040 | 1/6 | False |
| compare/durable_rollback_1row_10k/sqlite_wal | series2 | False | 1.281 | 3.549 | 5/6 | True |
| compare/update_1row_1k/lin | series1 | True | 19.027 | 1.613 | 6/6 | True |
| compare/update_1row_1k/lin | series2 | True | 20.267 | 2.599 | 6/6 | True |
| compare/update_1row_1k/sqlite | series1 | False | 2.649 | 6.564 | 5/6 | True |
| compare/update_1row_1k/sqlite | series2 | False | 2.268 | 2.543 | 4/6 | True |
| compare/delete_1row_1k/lin | series1 | False | -0.679 | 0.457 | 3/6 | True |
| compare/delete_1row_1k/lin | series2 | False | 3.102 | 3.711 | 5/6 | True |
| compare/delete_1row_1k/sqlite | series1 | False | 3.184 | 0.870 | 5/6 | True |
| compare/delete_1row_1k/sqlite | series2 | False | 1.584 | 1.858 | 3/6 | True |
| compare/update_1row_10k/lin | series1 | True | 13.941 | 1.771 | 6/6 | True |
| compare/update_1row_10k/lin | series2 | True | 15.577 | 1.195 | 6/6 | True |
| compare/update_1row_10k/sqlite | series1 | False | 3.144 | 3.990 | 5/6 | True |
| compare/update_1row_10k/sqlite | series2 | False | 0.141 | 2.850 | 3/6 | True |
| compare/delete_1row_10k/lin | series1 | False | 3.136 | 2.781 | 6/6 | True |
| compare/delete_1row_10k/lin | series2 | False | 3.974 | 3.221 | 6/6 | True |
| compare/delete_1row_10k/sqlite | series1 | False | -1.290 | 3.181 | 3/6 | True |
| compare/delete_1row_10k/sqlite | series2 | False | -1.201 | 5.855 | 2/6 | True |
| compare/update_1row_100k/lin | series1 | True | 20.253 | 4.414 | 6/6 | True |
| compare/update_1row_100k/lin | series2 | True | 20.150 | 6.662 | 6/6 | True |
| compare/update_1row_100k/sqlite | series1 | False | -8.082 | 9.048 | 1/6 | True |
| compare/update_1row_100k/sqlite | series2 | False | 0.124 | 2.114 | 3/6 | True |
| compare/delete_1row_100k/lin | series1 | False | 7.188 | 3.404 | 6/6 | True |
| compare/delete_1row_100k/lin | series2 | False | 8.030 | 3.866 | 6/6 | True |
| compare/delete_1row_100k/sqlite | series1 | False | -0.741 | 5.104 | 3/6 | True |
| compare/delete_1row_100k/sqlite | series2 | False | -0.453 | 2.907 | 2/6 | True |
| compare/tx_commit_1row_1k/lin | series1 | False | 18.987 | 1.774 | 6/6 | True |
| compare/tx_commit_1row_1k/lin | series2 | False | 16.652 | 1.928 | 6/6 | True |
| compare/tx_commit_1row_1k/sqlite | series1 | False | -2.157 | 6.640 | 2/6 | True |
| compare/tx_commit_1row_1k/sqlite | series2 | False | 0.746 | 5.939 | 3/6 | True |
| compare/tx_rollback_1row_1k/lin | series1 | False | 16.134 | 3.298 | 6/6 | True |
| compare/tx_rollback_1row_1k/lin | series2 | False | 19.258 | 1.218 | 6/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series1 | False | 3.515 | 1.496 | 5/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series2 | False | 2.273 | 2.959 | 4/6 | True |
| compare/tx_commit_1row_10k/lin | series1 | False | 15.244 | 2.012 | 6/6 | True |
| compare/tx_commit_1row_10k/lin | series2 | False | 15.339 | 2.563 | 6/6 | True |
| compare/tx_commit_1row_10k/sqlite | series1 | False | -1.268 | 4.845 | 0/6 | True |
| compare/tx_commit_1row_10k/sqlite | series2 | False | 0.162 | 3.472 | 3/6 | True |
| compare/tx_rollback_1row_10k/lin | series1 | False | 10.406 | 1.626 | 5/6 | True |
| compare/tx_rollback_1row_10k/lin | series2 | False | 12.406 | 2.255 | 6/6 | True |
| compare/tx_rollback_1row_10k/sqlite | series1 | False | 0.616 | 3.592 | 3/6 | True |
| compare/tx_rollback_1row_10k/sqlite | series2 | False | 0.788 | 1.970 | 4/6 | True |
| compare/insert_bulk_1k/lin | series1 | False | -1.690 | 0.573 | 1/6 | True |
| compare/insert_bulk_1k/lin | series2 | False | -1.521 | 1.501 | 0/6 | True |
| compare/insert_bulk_1k/sqlite | series1 | False | 0.682 | 0.597 | 5/6 | True |
| compare/insert_bulk_1k/sqlite | series2 | False | 1.083 | 1.243 | 4/6 | True |
| compare/insert_bulk_10k/lin | series1 | False | -5.683 | 2.125 | 0/6 | False |
| compare/insert_bulk_10k/lin | series2 | False | -7.712 | 1.203 | 0/6 | False |
| compare/insert_bulk_10k/sqlite | series1 | False | 0.260 | 1.172 | 4/6 | True |
| compare/insert_bulk_10k/sqlite | series2 | False | 0.842 | 0.418 | 5/6 | True |
| compare/durable_insert_1k/lin | series1 | False | 0.752 | 1.435 | 4/6 | True |
| compare/durable_insert_1k/lin | series2 | False | 0.675 | 1.186 | 5/6 | True |
| compare/durable_insert_1k/sqlite | series1 | False | 0.692 | 2.499 | 3/6 | True |
| compare/durable_insert_1k/sqlite | series2 | False | -0.303 | 2.443 | 3/6 | True |
| compare/durable_insert_10k/lin | series1 | False | 30.355 | 2.012 | 6/6 | True |
| compare/durable_insert_10k/lin | series2 | False | 28.893 | 2.585 | 6/6 | True |
| compare/durable_insert_10k/sqlite | series1 | False | 0.728 | 1.021 | 4/6 | True |
| compare/durable_insert_10k/sqlite | series2 | False | -0.232 | 1.027 | 2/6 | True |
| compare/filter_eq/lin | series1 | False | -5.243 | 0.643 | 0/6 | False |
| compare/filter_eq/lin | series2 | False | -0.420 | 3.411 | 3/6 | True |
| compare/filter_eq/sqlite | series1 | False | 0.750 | 1.118 | 5/6 | True |
| compare/filter_eq/sqlite | series2 | False | 0.439 | 1.814 | 4/6 | True |
| compare/filter_eq/duckdb | series1 | False | -0.271 | 0.654 | 1/6 | True |
| compare/filter_eq/duckdb | series2 | False | -0.362 | 5.628 | 3/6 | True |
| compare/filter_range/lin | series1 | False | 4.921 | 1.116 | 6/6 | True |
| compare/filter_range/lin | series2 | False | 4.421 | 2.289 | 6/6 | True |
| compare/filter_range/sqlite | series1 | False | -1.112 | 1.076 | 1/6 | True |
| compare/filter_range/sqlite | series2 | False | -0.230 | 0.718 | 2/6 | True |
| compare/filter_range/duckdb | series1 | False | 0.703 | 2.050 | 3/6 | True |
| compare/filter_range/duckdb | series2 | False | -0.692 | 3.432 | 3/6 | True |
| compare/agg_count_by/lin | series1 | False | 0.217 | 3.206 | 3/6 | True |
| compare/agg_count_by/lin | series2 | False | 3.660 | 5.444 | 6/6 | True |
| compare/agg_count_by/sqlite | series1 | False | -1.153 | 1.086 | 1/6 | True |
| compare/agg_count_by/sqlite | series2 | False | 0.086 | 0.833 | 3/6 | True |
| compare/agg_count_by/duckdb | series1 | False | -1.341 | 3.442 | 2/6 | True |
| compare/agg_count_by/duckdb | series2 | False | -4.235 | 4.849 | 1/6 | True |
| compare/wal_scalar_full_update/lin | series1 | True | 52.036 | 4.338 | 6/6 | True |
| compare/wal_scalar_full_update/lin | series2 | True | 52.999 | 2.847 | 6/6 | True |
| compare/wal_scalar_normal_update/lin | series1 | True | 94.651 | 1.321 | 6/6 | True |
| compare/wal_scalar_normal_update/lin | series2 | True | 94.680 | 0.934 | 6/6 | True |
| compare/tx_commit_1row_100k/lin | series1 | False | 19.485 | 4.367 | 6/6 | True |
| compare/tx_commit_1row_100k/lin | series2 | False | 21.663 | 5.485 | 6/6 | True |
| compare/tx_commit_1row_100k/sqlite | series1 | False | -5.048 | 0.943 | 1/6 | False |
| compare/tx_commit_1row_100k/sqlite | series2 | False | 0.208 | 5.096 | 3/6 | True |
| compare/tx_rollback_1row_100k/lin | series1 | False | 14.975 | 4.843 | 6/6 | True |
| compare/tx_rollback_1row_100k/lin | series2 | False | 17.812 | 2.051 | 6/6 | True |
| compare/tx_rollback_1row_100k/sqlite | series1 | False | -0.150 | 4.498 | 3/6 | True |
| compare/tx_rollback_1row_100k/sqlite | series2 | False | 3.259 | 4.151 | 5/6 | True |

| Lin case | Peer | Series | Lin/peer | Lin wins |
|---|---|---|---:|---:|
| compare/durable_update_1row_1k/lin | sqlite_delete | series1 | 0.3659 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series1 | 1.1384 | 1/6 |
| compare/durable_update_1row_1k/lin | sqlite_delete | series2 | 0.3922 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series2 | 1.1052 | 0/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series1 | 0.3964 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series1 | 1.2471 | 0/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series2 | 0.3880 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series2 | 1.1239 | 0/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series1 | 0.4049 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series1 | 1.1739 | 0/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series2 | 0.4195 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series2 | 1.2175 | 0/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series1 | 0.1984 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series1 | 1.2349 | 0/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series2 | 0.2169 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series2 | 1.3002 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series1 | 0.5092 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series1 | 1.6761 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series2 | 0.5982 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series2 | 1.6442 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series1 | 0.6252 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series1 | 1.7539 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series2 | 0.5469 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series2 | 1.7147 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series1 | 0.5999 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series1 | 1.6794 | 1/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series2 | 0.5897 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series2 | 1.7617 | 1/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series1 | 0.3353 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series1 | 1.7448 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series2 | 0.3315 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series2 | 1.7663 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series1 | 1.6446 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series2 | 1.7646 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series1 | 2.5671 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series2 | 2.5415 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series1 | 1.8868 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series2 | 1.8615 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series1 | 2.4262 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series2 | 2.4093 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series1 | 1.6996 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series2 | 1.7997 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series1 | 2.4143 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series2 | 2.5409 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series1 | 1.3717 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series2 | 1.3577 | 0/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series1 | 1.7842 | 0/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series2 | 1.7114 | 0/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series1 | 1.5060 | 0/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series2 | 1.5746 | 0/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series1 | 1.9305 | 0/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series2 | 1.9215 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series1 | 1.1995 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series2 | 1.1947 | 0/6 |
| compare/insert_bulk_10k/lin | sqlite | series1 | 1.0365 | 1/6 |
| compare/insert_bulk_10k/lin | sqlite | series2 | 1.0360 | 0/6 |
| compare/durable_insert_1k/lin | sqlite | series1 | 0.8572 | 6/6 |
| compare/durable_insert_1k/lin | sqlite | series2 | 0.8576 | 6/6 |
| compare/durable_insert_10k/lin | sqlite | series1 | 1.2485 | 0/6 |
| compare/durable_insert_10k/lin | sqlite | series2 | 1.2688 | 0/6 |
| compare/filter_eq/lin | sqlite | series1 | 0.0106 | 6/6 |
| compare/filter_eq/lin | duckdb | series1 | 0.0062 | 6/6 |
| compare/filter_eq/lin | sqlite | series2 | 0.0104 | 6/6 |
| compare/filter_eq/lin | duckdb | series2 | 0.0059 | 6/6 |
| compare/filter_range/lin | sqlite | series1 | 0.0142 | 6/6 |
| compare/filter_range/lin | duckdb | series1 | 0.0046 | 6/6 |
| compare/filter_range/lin | sqlite | series2 | 0.0144 | 6/6 |
| compare/filter_range/lin | duckdb | series2 | 0.0046 | 6/6 |
| compare/agg_count_by/lin | sqlite | series1 | 0.3009 | 6/6 |
| compare/agg_count_by/lin | duckdb | series1 | 0.2183 | 6/6 |
| compare/agg_count_by/lin | sqlite | series2 | 0.3000 | 6/6 |
| compare/agg_count_by/lin | duckdb | series2 | 0.2111 | 6/6 |
| compare/tx_commit_1row_100k/lin | sqlite | series1 | 1.5167 | 0/6 |
| compare/tx_commit_1row_100k/lin | sqlite | series2 | 1.4872 | 0/6 |
| compare/tx_rollback_1row_100k/lin | sqlite | series1 | 1.8655 | 0/6 |
| compare/tx_rollback_1row_100k/lin | sqlite | series2 | 1.7704 | 0/6 |

Source gate: False. Numeric peer wins: 184/456. Whole-process RSS in summary.json includes fixture, oracle and reopen; it is not isolated operation memory.
