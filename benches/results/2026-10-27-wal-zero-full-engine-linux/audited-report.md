# Positive-zero MessagePack WAL: full-engine paired gate

All 63 cases retained. Each series has six process pairs. Positive gain means candidate faster. Targets require 6/6 positive pairs and median gain above A/A. Controls require gain >= -max(2%, A/A). Full eight-peer objective remains false.

| Case | Series | Target | Gain % | A/A % | Positive | Pass |
|---|---|---|---:|---:|---:|---|
| compare/durable_update_1row_1k/lin | series1 | True | 4.355 | 12.489 | 3/6 | False |
| compare/durable_update_1row_1k/lin | series2 | True | 17.686 | 9.266 | 4/6 | False |
| compare/durable_update_1row_1k/sqlite_delete | series1 | False | 7.134 | 10.152 | 3/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series2 | False | -3.215 | 14.126 | 1/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series1 | False | -0.004 | 12.398 | 3/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series2 | False | 7.059 | 17.054 | 4/6 | True |
| compare/durable_delete_1row_1k/lin | series1 | True | 21.593 | 15.562 | 4/6 | False |
| compare/durable_delete_1row_1k/lin | series2 | True | 9.436 | 7.156 | 5/6 | False |
| compare/durable_delete_1row_1k/sqlite_delete | series1 | False | -0.146 | 25.752 | 3/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series2 | False | -4.222 | 15.662 | 2/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series1 | False | -6.147 | 20.528 | 2/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series2 | False | 0.442 | 4.371 | 3/6 | True |
| compare/durable_commit_1row_1k/lin | series1 | True | 30.161 | 17.187 | 6/6 | True |
| compare/durable_commit_1row_1k/lin | series2 | True | 8.942 | 12.515 | 5/6 | False |
| compare/durable_commit_1row_1k/sqlite_delete | series1 | False | -2.570 | 4.223 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series2 | False | -6.709 | 8.164 | 2/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series1 | False | -5.671 | 14.356 | 1/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series2 | False | -13.101 | 2.954 | 2/6 | False |
| compare/durable_rollback_1row_1k/lin | series1 | False | 3.525 | 6.610 | 4/6 | True |
| compare/durable_rollback_1row_1k/lin | series2 | False | -0.992 | 12.611 | 3/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series1 | False | -0.183 | 1.145 | 2/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series2 | False | 0.961 | 2.664 | 4/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series1 | False | -1.016 | 1.809 | 2/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series2 | False | -1.001 | 1.350 | 2/6 | True |
| compare/durable_update_1row_10k/lin | series1 | True | 17.764 | 13.841 | 5/6 | False |
| compare/durable_update_1row_10k/lin | series2 | True | 19.371 | 11.445 | 6/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series1 | False | 9.637 | 10.363 | 4/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series2 | False | -9.090 | 7.398 | 1/6 | False |
| compare/durable_update_1row_10k/sqlite_wal | series1 | False | -10.851 | 17.701 | 3/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series2 | False | -1.537 | 15.373 | 3/6 | True |
| compare/durable_delete_1row_10k/lin | series1 | True | 7.474 | 10.818 | 5/6 | False |
| compare/durable_delete_1row_10k/lin | series2 | True | 7.568 | 13.480 | 5/6 | False |
| compare/durable_delete_1row_10k/sqlite_delete | series1 | False | -1.196 | 17.731 | 3/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series2 | False | 11.442 | 5.616 | 5/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series1 | False | -18.540 | 18.489 | 2/6 | False |
| compare/durable_delete_1row_10k/sqlite_wal | series2 | False | -3.009 | 8.974 | 1/6 | True |
| compare/durable_commit_1row_10k/lin | series1 | True | 13.688 | 8.839 | 4/6 | False |
| compare/durable_commit_1row_10k/lin | series2 | True | 10.661 | 6.468 | 6/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series1 | False | -6.111 | 19.805 | 2/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series2 | False | 2.501 | 9.385 | 5/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series1 | False | -5.334 | 24.500 | 1/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series2 | False | 3.449 | 8.344 | 4/6 | True |
| compare/durable_rollback_1row_10k/lin | series1 | False | -0.856 | 2.231 | 2/6 | True |
| compare/durable_rollback_1row_10k/lin | series2 | False | 1.548 | 0.801 | 4/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series1 | False | -3.365 | 4.111 | 1/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series2 | False | -3.499 | 3.019 | 1/6 | False |
| compare/durable_rollback_1row_10k/sqlite_wal | series1 | False | 1.898 | 2.749 | 3/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series2 | False | 4.241 | 3.600 | 4/6 | True |
| compare/update_1row_1k/lin | series1 | False | -3.699 | 1.221 | 2/6 | False |
| compare/update_1row_1k/lin | series2 | False | 1.113 | 5.376 | 3/6 | True |
| compare/update_1row_1k/sqlite | series1 | False | -0.422 | 1.321 | 2/6 | True |
| compare/update_1row_1k/sqlite | series2 | False | 1.387 | 2.237 | 4/6 | True |
| compare/delete_1row_1k/lin | series1 | False | 1.588 | 2.964 | 4/6 | True |
| compare/delete_1row_1k/lin | series2 | False | 0.370 | 1.664 | 3/6 | True |
| compare/delete_1row_1k/sqlite | series1 | False | -0.395 | 1.730 | 2/6 | True |
| compare/delete_1row_1k/sqlite | series2 | False | 0.088 | 1.465 | 3/6 | True |
| compare/update_1row_10k/lin | series1 | False | 0.103 | 1.689 | 3/6 | True |
| compare/update_1row_10k/lin | series2 | False | -0.546 | 1.800 | 3/6 | True |
| compare/update_1row_10k/sqlite | series1 | False | 1.790 | 10.247 | 3/6 | True |
| compare/update_1row_10k/sqlite | series2 | False | -2.517 | 15.685 | 3/6 | True |
| compare/delete_1row_10k/lin | series1 | False | 0.080 | 2.618 | 3/6 | True |
| compare/delete_1row_10k/lin | series2 | False | 1.069 | 3.817 | 4/6 | True |
| compare/delete_1row_10k/sqlite | series1 | False | 4.862 | 8.297 | 4/6 | True |
| compare/delete_1row_10k/sqlite | series2 | False | -5.554 | 6.133 | 2/6 | True |
| compare/update_1row_100k/lin | series1 | False | -0.821 | 2.915 | 3/6 | True |
| compare/update_1row_100k/lin | series2 | False | -1.005 | 2.297 | 3/6 | True |
| compare/update_1row_100k/sqlite | series1 | False | 2.349 | 6.844 | 3/6 | True |
| compare/update_1row_100k/sqlite | series2 | False | 8.570 | 4.163 | 4/6 | True |
| compare/delete_1row_100k/lin | series1 | False | -1.499 | 4.019 | 2/6 | True |
| compare/delete_1row_100k/lin | series2 | False | 0.145 | 1.548 | 3/6 | True |
| compare/delete_1row_100k/sqlite | series1 | False | 3.326 | 7.220 | 3/6 | True |
| compare/delete_1row_100k/sqlite | series2 | False | -3.995 | 6.000 | 2/6 | True |
| compare/tx_commit_1row_1k/lin | series1 | False | 0.515 | 1.701 | 4/6 | True |
| compare/tx_commit_1row_1k/lin | series2 | False | -0.539 | 1.792 | 3/6 | True |
| compare/tx_commit_1row_1k/sqlite | series1 | False | -2.980 | 2.325 | 1/6 | False |
| compare/tx_commit_1row_1k/sqlite | series2 | False | 0.637 | 2.016 | 3/6 | True |
| compare/tx_rollback_1row_1k/lin | series1 | False | -0.858 | 1.525 | 3/6 | True |
| compare/tx_rollback_1row_1k/lin | series2 | False | -1.397 | 1.351 | 2/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series1 | False | 0.916 | 1.307 | 4/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series2 | False | -0.453 | 0.848 | 2/6 | True |
| compare/tx_commit_1row_10k/lin | series1 | False | 2.781 | 3.719 | 5/6 | True |
| compare/tx_commit_1row_10k/lin | series2 | False | 2.714 | 4.470 | 4/6 | True |
| compare/tx_commit_1row_10k/sqlite | series1 | False | -14.404 | 8.975 | 2/6 | False |
| compare/tx_commit_1row_10k/sqlite | series2 | False | -0.063 | 10.605 | 3/6 | True |
| compare/tx_rollback_1row_10k/lin | series1 | False | 0.900 | 2.015 | 3/6 | True |
| compare/tx_rollback_1row_10k/lin | series2 | False | -0.540 | 2.562 | 2/6 | True |
| compare/tx_rollback_1row_10k/sqlite | series1 | False | -0.906 | 8.164 | 3/6 | True |
| compare/tx_rollback_1row_10k/sqlite | series2 | False | -1.336 | 11.216 | 2/6 | True |
| compare/insert_bulk_1k/lin | series1 | False | -0.854 | 0.852 | 2/6 | True |
| compare/insert_bulk_1k/lin | series2 | False | -0.321 | 0.514 | 2/6 | True |
| compare/insert_bulk_1k/sqlite | series1 | False | 0.436 | 0.525 | 3/6 | True |
| compare/insert_bulk_1k/sqlite | series2 | False | 0.634 | 0.443 | 4/6 | True |
| compare/insert_bulk_10k/lin | series1 | False | -0.746 | 1.403 | 2/6 | True |
| compare/insert_bulk_10k/lin | series2 | False | 0.184 | 2.326 | 3/6 | True |
| compare/insert_bulk_10k/sqlite | series1 | False | -0.219 | 0.112 | 1/6 | True |
| compare/insert_bulk_10k/sqlite | series2 | False | 0.342 | 0.660 | 5/6 | True |
| compare/durable_insert_1k/lin | series1 | False | -1.122 | 1.925 | 2/6 | True |
| compare/durable_insert_1k/lin | series2 | False | 0.981 | 3.056 | 4/6 | True |
| compare/durable_insert_1k/sqlite | series1 | False | 1.594 | 5.881 | 3/6 | True |
| compare/durable_insert_1k/sqlite | series2 | False | 2.681 | 4.408 | 4/6 | True |
| compare/durable_insert_10k/lin | series1 | False | 0.069 | 0.829 | 3/6 | True |
| compare/durable_insert_10k/lin | series2 | False | -1.850 | 1.241 | 2/6 | True |
| compare/durable_insert_10k/sqlite | series1 | False | -0.043 | 1.045 | 3/6 | True |
| compare/durable_insert_10k/sqlite | series2 | False | 0.660 | 0.549 | 4/6 | True |
| compare/filter_eq/lin | series1 | False | -3.839 | 3.514 | 0/6 | False |
| compare/filter_eq/lin | series2 | False | -2.722 | 1.137 | 0/6 | False |
| compare/filter_eq/sqlite | series1 | False | -0.208 | 0.571 | 2/6 | True |
| compare/filter_eq/sqlite | series2 | False | -0.524 | 2.710 | 2/6 | True |
| compare/filter_eq/duckdb | series1 | False | 5.570 | 5.318 | 4/6 | True |
| compare/filter_eq/duckdb | series2 | False | -2.761 | 5.182 | 2/6 | True |
| compare/filter_range/lin | series1 | False | -2.892 | 1.663 | 1/6 | False |
| compare/filter_range/lin | series2 | False | -4.168 | 2.556 | 0/6 | False |
| compare/filter_range/sqlite | series1 | False | 0.345 | 1.149 | 5/6 | True |
| compare/filter_range/sqlite | series2 | False | -0.355 | 1.017 | 3/6 | True |
| compare/filter_range/duckdb | series1 | False | 0.861 | 5.389 | 5/6 | True |
| compare/filter_range/duckdb | series2 | False | 2.064 | 1.324 | 3/6 | True |
| compare/agg_count_by/lin | series1 | False | -1.018 | 0.717 | 1/6 | True |
| compare/agg_count_by/lin | series2 | False | -0.892 | 1.187 | 1/6 | True |
| compare/agg_count_by/sqlite | series1 | False | -0.743 | 0.389 | 2/6 | True |
| compare/agg_count_by/sqlite | series2 | False | 0.038 | 0.636 | 3/6 | True |
| compare/agg_count_by/duckdb | series1 | False | -0.700 | 2.296 | 3/6 | True |
| compare/agg_count_by/duckdb | series2 | False | -1.771 | 6.053 | 2/6 | True |
| compare/wal_scalar_full_update/lin | series1 | False | -5.522 | 8.277 | 2/6 | True |
| compare/wal_scalar_full_update/lin | series2 | False | 0.557 | 7.129 | 4/6 | True |
| compare/wal_scalar_normal_update/lin | series1 | False | -1.388 | 1.263 | 2/6 | True |
| compare/wal_scalar_normal_update/lin | series2 | False | -2.638 | 1.860 | 1/6 | False |

| Lin case | Peer | Series | Lin/peer | Lin wins |
|---|---|---|---:|---:|
| compare/durable_update_1row_1k/lin | sqlite_delete | series1 | 0.3802 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series1 | 1.0398 | 2/6 |
| compare/durable_update_1row_1k/lin | sqlite_delete | series2 | 0.4533 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series2 | 1.0776 | 2/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series1 | 0.2878 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series1 | 0.9955 | 3/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series2 | 0.3675 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series2 | 1.1411 | 2/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series1 | 0.3234 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series1 | 1.0266 | 2/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series2 | 0.3843 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series2 | 1.0925 | 3/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series1 | 0.2247 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series1 | 1.3528 | 0/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series2 | 0.2358 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series2 | 1.3910 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series1 | 0.4096 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series1 | 1.0121 | 3/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series2 | 0.4931 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series2 | 1.3126 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series1 | 0.4325 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series1 | 1.1877 | 1/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series2 | 0.5103 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series2 | 1.2600 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series1 | 0.4500 | 5/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series1 | 1.3100 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series2 | 0.4660 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series2 | 1.2572 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series1 | 0.3227 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series1 | 1.9238 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series2 | 0.3195 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series2 | 1.8743 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series1 | 1.7916 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series2 | 1.7436 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series1 | 2.0899 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series2 | 2.0087 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series1 | 2.3064 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series2 | 1.9137 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series1 | 2.5900 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series2 | 2.2941 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series1 | 1.7713 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series2 | 1.7027 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series1 | 2.1466 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series2 | 1.8645 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series1 | 1.3838 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series2 | 1.5024 | 0/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series1 | 1.8545 | 0/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series2 | 1.8933 | 0/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series1 | 1.6303 | 0/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series2 | 1.5601 | 0/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series1 | 1.8744 | 0/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series2 | 1.9219 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series1 | 1.1502 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series2 | 1.1608 | 0/6 |
| compare/insert_bulk_10k/lin | sqlite | series1 | 0.9799 | 6/6 |
| compare/insert_bulk_10k/lin | sqlite | series2 | 0.9717 | 5/6 |
| compare/durable_insert_1k/lin | sqlite | series1 | 0.8337 | 6/6 |
| compare/durable_insert_1k/lin | sqlite | series2 | 0.8605 | 6/6 |
| compare/durable_insert_10k/lin | sqlite | series1 | 1.4572 | 0/6 |
| compare/durable_insert_10k/lin | sqlite | series2 | 1.4982 | 0/6 |
| compare/filter_eq/lin | sqlite | series1 | 0.0100 | 6/6 |
| compare/filter_eq/lin | duckdb | series1 | 0.0052 | 6/6 |
| compare/filter_eq/lin | sqlite | series2 | 0.0101 | 6/6 |
| compare/filter_eq/lin | duckdb | series2 | 0.0052 | 6/6 |
| compare/filter_range/lin | sqlite | series1 | 0.0147 | 6/6 |
| compare/filter_range/lin | duckdb | series1 | 0.0051 | 6/6 |
| compare/filter_range/lin | sqlite | series2 | 0.0149 | 6/6 |
| compare/filter_range/lin | duckdb | series2 | 0.0052 | 6/6 |
| compare/agg_count_by/lin | sqlite | series1 | 0.3821 | 6/6 |
| compare/agg_count_by/lin | duckdb | series1 | 0.2696 | 6/6 |
| compare/agg_count_by/lin | sqlite | series2 | 0.3862 | 6/6 |
| compare/agg_count_by/lin | duckdb | series2 | 0.2647 | 6/6 |

Source gate: False. Numeric peer wins: 208/432. Whole-process RSS in summary.json includes fixture, oracle and reopen; it is not isolated operation memory.

This display heading corrects a retained generator label. The original CI report and raw data remain unchanged in `wal-zero-full-engine/report.md`. CPU component acceptance remains false; this report evaluates the separate end-to-end I/O hypothesis.
