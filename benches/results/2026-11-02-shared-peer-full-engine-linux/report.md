# Corrected COW with invariant external peer executable: full-engine gate

All67 cases retained. External peers use the identical baseline-built executable for every role; Lin uses its source role. Prior linked-peer failures are not reclassified. Each series has six process pairs. Positive gain means candidate faster. Targets require 6/6 positive pairs and median gain above A/A. Controls require gain >= -max(2%, A/A). Full eight-peer objective remains false.

| Case | Series | Target | Gain % | A/A % | Positive | Pass |
|---|---|---|---:|---:|---:|---|
| compare/durable_update_1row_1k/lin | series1 | False | 2.359 | 6.407 | 4/6 | True |
| compare/durable_update_1row_1k/lin | series2 | False | 2.843 | 3.518 | 5/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series1 | False | -1.326 | 3.420 | 2/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series2 | False | -5.526 | 5.907 | 2/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series1 | False | -4.417 | 4.675 | 2/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series2 | False | 1.013 | 3.482 | 4/6 | True |
| compare/durable_delete_1row_1k/lin | series1 | False | -4.811 | 3.020 | 2/6 | False |
| compare/durable_delete_1row_1k/lin | series2 | False | 0.047 | 7.798 | 3/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series1 | False | -3.833 | 5.206 | 1/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series2 | False | -0.674 | 3.166 | 3/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series1 | False | -2.307 | 2.820 | 2/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series2 | False | -0.440 | 4.408 | 3/6 | True |
| compare/durable_commit_1row_1k/lin | series1 | False | -0.062 | 6.918 | 3/6 | True |
| compare/durable_commit_1row_1k/lin | series2 | False | -2.334 | 5.255 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series1 | False | 0.221 | 3.977 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series2 | False | -0.445 | 4.205 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series1 | False | -5.179 | 3.531 | 2/6 | False |
| compare/durable_commit_1row_1k/sqlite_wal | series2 | False | -2.106 | 1.090 | 2/6 | False |
| compare/durable_rollback_1row_1k/lin | series1 | False | 16.786 | 5.757 | 5/6 | True |
| compare/durable_rollback_1row_1k/lin | series2 | False | 7.127 | 8.075 | 6/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series1 | False | 0.264 | 4.862 | 3/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series2 | False | 2.671 | 3.222 | 6/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series1 | False | -0.881 | 2.339 | 1/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series2 | False | -1.242 | 3.148 | 2/6 | True |
| compare/durable_update_1row_10k/lin | series1 | False | 2.432 | 4.530 | 5/6 | True |
| compare/durable_update_1row_10k/lin | series2 | False | 3.083 | 3.845 | 4/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series1 | False | 0.049 | 4.150 | 3/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series2 | False | 3.817 | 3.342 | 4/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series1 | False | 3.622 | 4.806 | 4/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series2 | False | -2.284 | 2.508 | 2/6 | True |
| compare/durable_delete_1row_10k/lin | series1 | False | 0.602 | 4.404 | 5/6 | True |
| compare/durable_delete_1row_10k/lin | series2 | False | 2.966 | 4.985 | 4/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series1 | False | -2.411 | 4.058 | 2/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series2 | False | 0.054 | 2.732 | 3/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series1 | False | 2.203 | 7.908 | 4/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series2 | False | -2.326 | 2.908 | 3/6 | True |
| compare/durable_commit_1row_10k/lin | series1 | False | 1.389 | 4.668 | 4/6 | True |
| compare/durable_commit_1row_10k/lin | series2 | False | 4.759 | 5.680 | 4/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series1 | False | -0.176 | 16.843 | 3/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series2 | False | -0.395 | 5.034 | 3/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series1 | False | 1.284 | 4.744 | 3/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series2 | False | 1.549 | 4.212 | 5/6 | True |
| compare/durable_rollback_1row_10k/lin | series1 | False | 9.192 | 3.132 | 6/6 | True |
| compare/durable_rollback_1row_10k/lin | series2 | False | 8.618 | 2.425 | 6/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series1 | False | 5.868 | 3.829 | 4/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series2 | False | 0.223 | 5.799 | 3/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series1 | False | 2.409 | 7.445 | 4/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series2 | False | 1.747 | 3.057 | 4/6 | True |
| compare/update_1row_1k/lin | series1 | True | 53.790 | 5.120 | 6/6 | True |
| compare/update_1row_1k/lin | series2 | True | 70.528 | 4.054 | 6/6 | True |
| compare/update_1row_1k/sqlite | series1 | False | 2.283 | 3.896 | 4/6 | True |
| compare/update_1row_1k/sqlite | series2 | False | 1.970 | 3.129 | 4/6 | True |
| compare/delete_1row_1k/lin | series1 | False | -4.039 | 4.656 | 2/6 | True |
| compare/delete_1row_1k/lin | series2 | False | -0.890 | 4.512 | 2/6 | True |
| compare/delete_1row_1k/sqlite | series1 | False | 3.207 | 4.255 | 5/6 | True |
| compare/delete_1row_1k/sqlite | series2 | False | -2.167 | 4.746 | 3/6 | True |
| compare/update_1row_10k/lin | series1 | True | 37.148 | 3.026 | 6/6 | True |
| compare/update_1row_10k/lin | series2 | True | 40.775 | 3.212 | 6/6 | True |
| compare/update_1row_10k/sqlite | series1 | False | 8.698 | 8.604 | 6/6 | True |
| compare/update_1row_10k/sqlite | series2 | False | 5.679 | 7.788 | 6/6 | True |
| compare/delete_1row_10k/lin | series1 | False | 0.885 | 3.247 | 3/6 | True |
| compare/delete_1row_10k/lin | series2 | False | 0.594 | 2.981 | 3/6 | True |
| compare/delete_1row_10k/sqlite | series1 | False | -4.728 | 13.672 | 2/6 | True |
| compare/delete_1row_10k/sqlite | series2 | False | -8.616 | 9.530 | 2/6 | True |
| compare/update_1row_100k/lin | series1 | True | 41.531 | 6.316 | 6/6 | True |
| compare/update_1row_100k/lin | series2 | True | 40.275 | 3.799 | 6/6 | True |
| compare/update_1row_100k/sqlite | series1 | False | -9.003 | 5.525 | 2/6 | False |
| compare/update_1row_100k/sqlite | series2 | False | -12.954 | 9.999 | 2/6 | False |
| compare/delete_1row_100k/lin | series1 | False | 1.157 | 5.674 | 3/6 | True |
| compare/delete_1row_100k/lin | series2 | False | 1.269 | 5.253 | 4/6 | True |
| compare/delete_1row_100k/sqlite | series1 | False | 6.745 | 13.179 | 5/6 | True |
| compare/delete_1row_100k/sqlite | series2 | False | 2.039 | 9.923 | 3/6 | True |
| compare/tx_commit_1row_1k/lin | series1 | False | 67.709 | 5.642 | 6/6 | True |
| compare/tx_commit_1row_1k/lin | series2 | False | 67.369 | 4.610 | 6/6 | True |
| compare/tx_commit_1row_1k/sqlite | series1 | False | 2.048 | 3.618 | 4/6 | True |
| compare/tx_commit_1row_1k/sqlite | series2 | False | 4.071 | 5.465 | 6/6 | True |
| compare/tx_rollback_1row_1k/lin | series1 | False | 53.410 | 1.942 | 6/6 | True |
| compare/tx_rollback_1row_1k/lin | series2 | False | 54.405 | 5.225 | 6/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series1 | False | -5.964 | 2.835 | 1/6 | False |
| compare/tx_rollback_1row_1k/sqlite | series2 | False | -1.975 | 7.480 | 1/6 | True |
| compare/tx_commit_1row_10k/lin | series1 | False | 37.899 | 3.274 | 6/6 | True |
| compare/tx_commit_1row_10k/lin | series2 | False | 39.670 | 1.235 | 6/6 | True |
| compare/tx_commit_1row_10k/sqlite | series1 | False | -2.093 | 3.395 | 2/6 | True |
| compare/tx_commit_1row_10k/sqlite | series2 | False | -0.363 | 2.731 | 2/6 | True |
| compare/tx_rollback_1row_10k/lin | series1 | False | 30.783 | 2.146 | 6/6 | True |
| compare/tx_rollback_1row_10k/lin | series2 | False | 27.700 | 1.833 | 6/6 | True |
| compare/tx_rollback_1row_10k/sqlite | series1 | False | 1.451 | 20.569 | 3/6 | True |
| compare/tx_rollback_1row_10k/sqlite | series2 | False | -0.406 | 14.349 | 3/6 | True |
| compare/insert_bulk_1k/lin | series1 | False | -1.237 | 2.349 | 1/6 | True |
| compare/insert_bulk_1k/lin | series2 | False | -1.148 | 2.538 | 1/6 | True |
| compare/insert_bulk_1k/sqlite | series1 | False | -1.172 | 1.630 | 1/6 | True |
| compare/insert_bulk_1k/sqlite | series2 | False | 0.787 | 1.080 | 3/6 | True |
| compare/insert_bulk_10k/lin | series1 | False | -2.291 | 3.873 | 0/6 | True |
| compare/insert_bulk_10k/lin | series2 | False | -3.573 | 2.904 | 0/6 | False |
| compare/insert_bulk_10k/sqlite | series1 | False | 1.420 | 4.307 | 5/6 | True |
| compare/insert_bulk_10k/sqlite | series2 | False | 2.228 | 3.483 | 5/6 | True |
| compare/durable_insert_1k/lin | series1 | False | -2.559 | 5.588 | 2/6 | True |
| compare/durable_insert_1k/lin | series2 | False | 1.586 | 2.062 | 4/6 | True |
| compare/durable_insert_1k/sqlite | series1 | False | 3.839 | 4.835 | 4/6 | True |
| compare/durable_insert_1k/sqlite | series2 | False | -0.359 | 2.386 | 3/6 | True |
| compare/durable_insert_10k/lin | series1 | False | 4.398 | 13.499 | 4/6 | True |
| compare/durable_insert_10k/lin | series2 | False | -1.308 | 4.277 | 2/6 | True |
| compare/durable_insert_10k/sqlite | series1 | False | 0.570 | 2.966 | 4/6 | True |
| compare/durable_insert_10k/sqlite | series2 | False | -1.093 | 1.486 | 2/6 | True |
| compare/filter_eq/lin | series1 | False | 3.836 | 2.260 | 5/6 | True |
| compare/filter_eq/lin | series2 | False | 3.242 | 1.100 | 6/6 | True |
| compare/filter_eq/sqlite | series1 | False | 2.710 | 3.484 | 4/6 | True |
| compare/filter_eq/sqlite | series2 | False | 0.534 | 1.619 | 4/6 | True |
| compare/filter_eq/duckdb | series1 | False | -10.511 | 11.865 | 1/6 | True |
| compare/filter_eq/duckdb | series2 | False | -8.472 | 10.173 | 2/6 | True |
| compare/filter_range/lin | series1 | False | 7.931 | 1.899 | 6/6 | True |
| compare/filter_range/lin | series2 | False | 6.131 | 1.658 | 6/6 | True |
| compare/filter_range/sqlite | series1 | False | 0.450 | 3.429 | 3/6 | True |
| compare/filter_range/sqlite | series2 | False | -0.555 | 1.440 | 2/6 | True |
| compare/filter_range/duckdb | series1 | False | -4.612 | 4.693 | 2/6 | True |
| compare/filter_range/duckdb | series2 | False | 7.561 | 8.711 | 5/6 | True |
| compare/agg_count_by/lin | series1 | False | 0.156 | 1.761 | 4/6 | True |
| compare/agg_count_by/lin | series2 | False | 0.431 | 0.732 | 4/6 | True |
| compare/agg_count_by/sqlite | series1 | False | -0.532 | 1.561 | 3/6 | True |
| compare/agg_count_by/sqlite | series2 | False | 0.477 | 3.639 | 4/6 | True |
| compare/agg_count_by/duckdb | series1 | False | 1.237 | 4.355 | 3/6 | True |
| compare/agg_count_by/duckdb | series2 | False | -0.096 | 5.050 | 3/6 | True |
| compare/wal_scalar_full_update/lin | series1 | True | 44.392 | 3.308 | 6/6 | True |
| compare/wal_scalar_full_update/lin | series2 | True | 44.538 | 4.088 | 6/6 | True |
| compare/wal_scalar_normal_update/lin | series1 | True | 96.311 | 0.944 | 6/6 | True |
| compare/wal_scalar_normal_update/lin | series2 | True | 96.211 | 2.993 | 6/6 | True |
| compare/tx_commit_1row_100k/lin | series1 | False | 39.001 | 9.720 | 6/6 | True |
| compare/tx_commit_1row_100k/lin | series2 | False | 42.276 | 5.312 | 6/6 | True |
| compare/tx_commit_1row_100k/sqlite | series1 | False | -0.516 | 7.870 | 2/6 | True |
| compare/tx_commit_1row_100k/sqlite | series2 | False | 2.215 | 6.524 | 5/6 | True |
| compare/tx_rollback_1row_100k/lin | series1 | False | 31.216 | 5.616 | 6/6 | True |
| compare/tx_rollback_1row_100k/lin | series2 | False | 34.100 | 3.743 | 6/6 | True |
| compare/tx_rollback_1row_100k/sqlite | series1 | False | 1.557 | 4.457 | 3/6 | True |
| compare/tx_rollback_1row_100k/sqlite | series2 | False | -1.823 | 5.473 | 1/6 | True |

| Lin case | Peer | Series | Lin/peer | Lin wins |
|---|---|---|---:|---:|
| compare/durable_update_1row_1k/lin | sqlite_delete | series1 | 0.3806 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series1 | 1.1542 | 0/6 |
| compare/durable_update_1row_1k/lin | sqlite_delete | series2 | 0.4187 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series2 | 1.2688 | 0/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series1 | 0.3957 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series1 | 1.1933 | 0/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series2 | 0.4533 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series2 | 1.2116 | 0/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series1 | 0.3876 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series1 | 1.1248 | 0/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series2 | 0.4434 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series2 | 1.2471 | 0/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series1 | 0.3212 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series1 | 1.9498 | 0/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series2 | 0.3679 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series2 | 2.3317 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series1 | 0.5943 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series1 | 1.7969 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series2 | 0.6326 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series2 | 1.7960 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series1 | 0.6118 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series1 | 1.8363 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series2 | 0.6077 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series2 | 1.7125 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series1 | 0.6013 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series1 | 1.8498 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series2 | 0.5785 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series2 | 1.6962 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series1 | 0.4387 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series1 | 2.3248 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series2 | 0.4732 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series2 | 2.5698 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series1 | 1.2234 | 2/6 |
| compare/update_1row_1k/lin | sqlite | series2 | 0.7302 | 6/6 |
| compare/delete_1row_1k/lin | sqlite | series1 | 2.8106 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series2 | 2.5766 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series1 | 1.6295 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series2 | 2.1538 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series1 | 2.8820 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series2 | 2.8049 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series1 | 0.8514 | 5/6 |
| compare/update_1row_100k/lin | sqlite | series2 | 0.8594 | 4/6 |
| compare/delete_1row_100k/lin | sqlite | series1 | 2.1987 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series2 | 1.8851 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series1 | 0.6086 | 6/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series2 | 0.5704 | 6/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series1 | 1.0479 | 1/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series2 | 1.0522 | 2/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series1 | 1.4743 | 1/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series2 | 1.9490 | 1/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series1 | 2.3783 | 0/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series2 | 2.2934 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series1 | 1.2954 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series2 | 1.3157 | 0/6 |
| compare/insert_bulk_10k/lin | sqlite | series1 | 1.1247 | 0/6 |
| compare/insert_bulk_10k/lin | sqlite | series2 | 1.1626 | 0/6 |
| compare/durable_insert_1k/lin | sqlite | series1 | 0.8740 | 6/6 |
| compare/durable_insert_1k/lin | sqlite | series2 | 0.8603 | 6/6 |
| compare/durable_insert_10k/lin | sqlite | series1 | 1.5255 | 0/6 |
| compare/durable_insert_10k/lin | sqlite | series2 | 1.5832 | 0/6 |
| compare/filter_eq/lin | sqlite | series1 | 0.0112 | 6/6 |
| compare/filter_eq/lin | duckdb | series1 | 0.0062 | 6/6 |
| compare/filter_eq/lin | sqlite | series2 | 0.0110 | 6/6 |
| compare/filter_eq/lin | duckdb | series2 | 0.0063 | 6/6 |
| compare/filter_range/lin | sqlite | series1 | 0.0156 | 6/6 |
| compare/filter_range/lin | duckdb | series1 | 0.0041 | 6/6 |
| compare/filter_range/lin | sqlite | series2 | 0.0157 | 6/6 |
| compare/filter_range/lin | duckdb | series2 | 0.0046 | 6/6 |
| compare/agg_count_by/lin | sqlite | series1 | 0.5679 | 6/6 |
| compare/agg_count_by/lin | duckdb | series1 | 0.3518 | 6/6 |
| compare/agg_count_by/lin | sqlite | series2 | 0.5784 | 6/6 |
| compare/agg_count_by/lin | duckdb | series2 | 0.3363 | 6/6 |
| compare/tx_commit_1row_100k/lin | sqlite | series1 | 0.8270 | 4/6 |
| compare/tx_commit_1row_100k/lin | sqlite | series2 | 0.8348 | 5/6 |
| compare/tx_rollback_1row_100k/lin | sqlite | series1 | 1.2710 | 0/6 |
| compare/tx_rollback_1row_100k/lin | sqlite | series2 | 1.0263 | 3/6 |

Source gate: False. Numeric peer wins: 226/456. Whole-process RSS in summary.json includes fixture, oracle and reopen; it is not isolated operation memory.
