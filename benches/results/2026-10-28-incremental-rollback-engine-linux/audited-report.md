# Incremental owned rollback: full-engine paired gate

All 67 cases retained. Each series has six process pairs. Positive gain means candidate faster. Targets require 6/6 positive pairs and median gain above A/A. Controls require gain >= -max(2%, A/A). Full eight-peer objective remains false.

| Case | Series | Target | Gain % | A/A % | Positive | Pass |
|---|---|---|---:|---:|---:|---|
| compare/durable_update_1row_1k/lin | series1 | False | -1.101 | 6.666 | 3/6 | True |
| compare/durable_update_1row_1k/lin | series2 | False | -2.265 | 4.011 | 3/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series1 | False | -3.109 | 8.904 | 1/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series2 | False | -4.321 | 16.294 | 2/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series1 | False | 0.205 | 3.469 | 3/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series2 | False | -6.663 | 2.901 | 0/6 | False |
| compare/durable_delete_1row_1k/lin | series1 | False | -0.071 | 9.318 | 3/6 | True |
| compare/durable_delete_1row_1k/lin | series2 | False | 4.537 | 5.508 | 4/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series1 | False | 1.521 | 10.385 | 3/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series2 | False | 1.294 | 5.479 | 3/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series1 | False | -4.327 | 6.435 | 1/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series2 | False | -1.107 | 4.977 | 3/6 | True |
| compare/durable_commit_1row_1k/lin | series1 | False | -4.031 | 5.378 | 2/6 | True |
| compare/durable_commit_1row_1k/lin | series2 | False | 1.036 | 2.287 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series1 | False | -16.023 | 13.648 | 1/6 | False |
| compare/durable_commit_1row_1k/sqlite_delete | series2 | False | -9.191 | 10.027 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series1 | False | 0.836 | 3.139 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series2 | False | -1.364 | 4.426 | 2/6 | True |
| compare/durable_rollback_1row_1k/lin | series1 | True | 25.656 | 1.015 | 6/6 | True |
| compare/durable_rollback_1row_1k/lin | series2 | True | 25.089 | 1.301 | 6/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series1 | False | 0.264 | 3.784 | 3/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series2 | False | 0.561 | 3.728 | 4/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series1 | False | 0.265 | 0.916 | 3/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series2 | False | -0.519 | 2.258 | 3/6 | True |
| compare/durable_update_1row_10k/lin | series1 | False | -4.320 | 13.133 | 3/6 | True |
| compare/durable_update_1row_10k/lin | series2 | False | -14.910 | 14.184 | 2/6 | False |
| compare/durable_update_1row_10k/sqlite_delete | series1 | False | 0.404 | 4.692 | 3/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series2 | False | -9.104 | 20.880 | 0/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series1 | False | -1.560 | 3.921 | 3/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series2 | False | 1.095 | 2.052 | 4/6 | True |
| compare/durable_delete_1row_10k/lin | series1 | False | -19.404 | 11.506 | 0/6 | False |
| compare/durable_delete_1row_10k/lin | series2 | False | -7.209 | 7.827 | 3/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series1 | False | 6.227 | 5.135 | 4/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series2 | False | -1.799 | 4.172 | 3/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series1 | False | 3.353 | 1.939 | 4/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series2 | False | -1.251 | 2.400 | 2/6 | True |
| compare/durable_commit_1row_10k/lin | series1 | False | 1.035 | 15.949 | 3/6 | True |
| compare/durable_commit_1row_10k/lin | series2 | False | -13.232 | 6.033 | 1/6 | False |
| compare/durable_commit_1row_10k/sqlite_delete | series1 | False | -6.820 | 2.816 | 2/6 | False |
| compare/durable_commit_1row_10k/sqlite_delete | series2 | False | 0.654 | 19.597 | 3/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series1 | False | -3.695 | 3.594 | 2/6 | False |
| compare/durable_commit_1row_10k/sqlite_wal | series2 | False | -1.272 | 1.973 | 2/6 | True |
| compare/durable_rollback_1row_10k/lin | series1 | True | 21.759 | 2.119 | 6/6 | True |
| compare/durable_rollback_1row_10k/lin | series2 | True | 21.523 | 3.713 | 6/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series1 | False | -0.059 | 1.806 | 3/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series2 | False | -0.395 | 1.525 | 3/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series1 | False | 0.173 | 2.843 | 5/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series2 | False | -1.471 | 3.243 | 1/6 | True |
| compare/update_1row_1k/lin | series1 | False | 1.209 | 1.739 | 3/6 | True |
| compare/update_1row_1k/lin | series2 | False | 0.415 | 1.842 | 3/6 | True |
| compare/update_1row_1k/sqlite | series1 | False | -1.977 | 1.723 | 1/6 | True |
| compare/update_1row_1k/sqlite | series2 | False | -0.356 | 1.895 | 3/6 | True |
| compare/delete_1row_1k/lin | series1 | False | 0.392 | 1.921 | 3/6 | True |
| compare/delete_1row_1k/lin | series2 | False | 1.617 | 3.460 | 4/6 | True |
| compare/delete_1row_1k/sqlite | series1 | False | -0.002 | 4.297 | 3/6 | True |
| compare/delete_1row_1k/sqlite | series2 | False | -0.584 | 2.074 | 2/6 | True |
| compare/update_1row_10k/lin | series1 | False | -0.716 | 2.106 | 2/6 | True |
| compare/update_1row_10k/lin | series2 | False | 1.056 | 2.493 | 4/6 | True |
| compare/update_1row_10k/sqlite | series1 | False | 2.414 | 5.770 | 4/6 | True |
| compare/update_1row_10k/sqlite | series2 | False | -1.296 | 6.186 | 1/6 | True |
| compare/delete_1row_10k/lin | series1 | False | 0.966 | 0.184 | 4/6 | True |
| compare/delete_1row_10k/lin | series2 | False | 0.246 | 2.232 | 3/6 | True |
| compare/delete_1row_10k/sqlite | series1 | False | -1.234 | 6.184 | 3/6 | True |
| compare/delete_1row_10k/sqlite | series2 | False | 2.902 | 1.268 | 4/6 | True |
| compare/update_1row_100k/lin | series1 | False | -0.710 | 1.048 | 2/6 | True |
| compare/update_1row_100k/lin | series2 | False | -1.415 | 0.370 | 2/6 | True |
| compare/update_1row_100k/sqlite | series1 | False | 0.094 | 1.992 | 3/6 | True |
| compare/update_1row_100k/sqlite | series2 | False | -0.606 | 3.703 | 3/6 | True |
| compare/delete_1row_100k/lin | series1 | False | 1.176 | 1.452 | 5/6 | True |
| compare/delete_1row_100k/lin | series2 | False | 0.091 | 2.461 | 3/6 | True |
| compare/delete_1row_100k/sqlite | series1 | False | 6.612 | 7.758 | 4/6 | True |
| compare/delete_1row_100k/sqlite | series2 | False | 1.875 | 4.326 | 3/6 | True |
| compare/tx_commit_1row_1k/lin | series1 | False | -2.483 | 2.013 | 0/6 | False |
| compare/tx_commit_1row_1k/lin | series2 | False | 1.809 | 1.481 | 5/6 | True |
| compare/tx_commit_1row_1k/sqlite | series1 | False | -1.640 | 2.290 | 3/6 | True |
| compare/tx_commit_1row_1k/sqlite | series2 | False | -0.883 | 3.617 | 2/6 | True |
| compare/tx_rollback_1row_1k/lin | series1 | True | 22.530 | 3.108 | 6/6 | True |
| compare/tx_rollback_1row_1k/lin | series2 | True | 20.108 | 2.220 | 6/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series1 | False | -0.373 | 3.343 | 3/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series2 | False | -3.611 | 1.789 | 1/6 | False |
| compare/tx_commit_1row_10k/lin | series1 | False | -1.166 | 2.187 | 2/6 | True |
| compare/tx_commit_1row_10k/lin | series2 | False | -0.332 | 1.507 | 3/6 | True |
| compare/tx_commit_1row_10k/sqlite | series1 | False | -0.420 | 3.109 | 3/6 | True |
| compare/tx_commit_1row_10k/sqlite | series2 | False | 1.831 | 2.686 | 5/6 | True |
| compare/tx_rollback_1row_10k/lin | series1 | True | 18.590 | 0.686 | 6/6 | True |
| compare/tx_rollback_1row_10k/lin | series2 | True | 18.756 | 0.987 | 6/6 | True |
| compare/tx_rollback_1row_10k/sqlite | series1 | False | -2.205 | 1.794 | 2/6 | False |
| compare/tx_rollback_1row_10k/sqlite | series2 | False | 1.109 | 2.360 | 4/6 | True |
| compare/insert_bulk_1k/lin | series1 | False | -3.436 | 1.306 | 1/6 | False |
| compare/insert_bulk_1k/lin | series2 | False | 0.521 | 4.589 | 3/6 | True |
| compare/insert_bulk_1k/sqlite | series1 | False | -0.474 | 0.194 | 2/6 | True |
| compare/insert_bulk_1k/sqlite | series2 | False | -0.504 | 0.471 | 2/6 | True |
| compare/insert_bulk_10k/lin | series1 | False | -2.171 | 0.703 | 0/6 | False |
| compare/insert_bulk_10k/lin | series2 | False | -1.072 | 0.561 | 1/6 | True |
| compare/insert_bulk_10k/sqlite | series1 | False | -0.031 | 0.192 | 2/6 | True |
| compare/insert_bulk_10k/sqlite | series2 | False | -0.209 | 0.157 | 1/6 | True |
| compare/durable_insert_1k/lin | series1 | False | 3.941 | 3.109 | 6/6 | True |
| compare/durable_insert_1k/lin | series2 | False | -0.283 | 3.231 | 3/6 | True |
| compare/durable_insert_1k/sqlite | series1 | False | -1.603 | 2.277 | 2/6 | True |
| compare/durable_insert_1k/sqlite | series2 | False | 2.360 | 3.629 | 4/6 | True |
| compare/durable_insert_10k/lin | series1 | False | -2.159 | 1.744 | 2/6 | False |
| compare/durable_insert_10k/lin | series2 | False | -0.423 | 0.806 | 2/6 | True |
| compare/durable_insert_10k/sqlite | series1 | False | -0.155 | 0.927 | 2/6 | True |
| compare/durable_insert_10k/sqlite | series2 | False | 0.375 | 0.514 | 4/6 | True |
| compare/filter_eq/lin | series1 | False | 2.807 | 8.492 | 4/6 | True |
| compare/filter_eq/lin | series2 | False | -1.056 | 5.949 | 3/6 | True |
| compare/filter_eq/sqlite | series1 | False | -0.225 | 0.578 | 3/6 | True |
| compare/filter_eq/sqlite | series2 | False | -0.421 | 0.551 | 2/6 | True |
| compare/filter_eq/duckdb | series1 | False | -4.141 | 2.924 | 3/6 | False |
| compare/filter_eq/duckdb | series2 | False | 5.367 | 5.104 | 4/6 | True |
| compare/filter_range/lin | series1 | False | 2.294 | 4.650 | 4/6 | True |
| compare/filter_range/lin | series2 | False | 4.793 | 4.254 | 4/6 | True |
| compare/filter_range/sqlite | series1 | False | -0.257 | 1.561 | 2/6 | True |
| compare/filter_range/sqlite | series2 | False | -0.062 | 0.728 | 3/6 | True |
| compare/filter_range/duckdb | series1 | False | -1.548 | 6.191 | 2/6 | True |
| compare/filter_range/duckdb | series2 | False | 1.232 | 3.435 | 4/6 | True |
| compare/agg_count_by/lin | series1 | False | -0.886 | 0.518 | 1/6 | True |
| compare/agg_count_by/lin | series2 | False | -0.416 | 1.840 | 2/6 | True |
| compare/agg_count_by/sqlite | series1 | False | -0.620 | 1.535 | 2/6 | True |
| compare/agg_count_by/sqlite | series2 | False | -0.272 | 1.393 | 3/6 | True |
| compare/agg_count_by/duckdb | series1 | False | 8.081 | 5.179 | 4/6 | True |
| compare/agg_count_by/duckdb | series2 | False | -0.739 | 5.189 | 2/6 | True |
| compare/wal_scalar_full_update/lin | series1 | False | -1.066 | 2.994 | 3/6 | True |
| compare/wal_scalar_full_update/lin | series2 | False | -0.792 | 3.596 | 1/6 | True |
| compare/wal_scalar_normal_update/lin | series1 | False | 0.783 | 1.742 | 5/6 | True |
| compare/wal_scalar_normal_update/lin | series2 | False | -0.596 | 0.413 | 1/6 | True |
| compare/tx_commit_1row_100k/lin | series1 | False | -2.358 | 3.441 | 2/6 | True |
| compare/tx_commit_1row_100k/lin | series2 | False | 0.784 | 4.912 | 3/6 | True |
| compare/tx_commit_1row_100k/sqlite | series1 | False | -2.123 | 7.141 | 3/6 | True |
| compare/tx_commit_1row_100k/sqlite | series2 | False | 2.426 | 3.918 | 3/6 | True |
| compare/tx_rollback_1row_100k/lin | series1 | True | 16.489 | 1.291 | 6/6 | True |
| compare/tx_rollback_1row_100k/lin | series2 | True | 18.472 | 2.329 | 6/6 | True |
| compare/tx_rollback_1row_100k/sqlite | series1 | False | 1.135 | 2.984 | 5/6 | True |
| compare/tx_rollback_1row_100k/sqlite | series2 | False | 1.001 | 2.985 | 5/6 | True |

| Lin case | Peer | Series | Lin/peer | Lin wins |
|---|---|---|---:|---:|
| compare/durable_update_1row_1k/lin | sqlite_delete | series1 | 0.3115 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series1 | 1.0940 | 1/6 |
| compare/durable_update_1row_1k/lin | sqlite_delete | series2 | 0.2730 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series2 | 1.0187 | 3/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series1 | 0.3061 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series1 | 1.0672 | 1/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series2 | 0.2990 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series2 | 1.0511 | 1/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series1 | 0.2981 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series1 | 1.0809 | 0/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series2 | 0.2916 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series2 | 1.0247 | 2/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series1 | 0.3138 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series1 | 1.3274 | 0/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series2 | 0.3197 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series2 | 1.3277 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series1 | 0.3571 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series1 | 1.3958 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series2 | 0.4145 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series2 | 1.6289 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series1 | 0.4674 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series1 | 1.5627 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series2 | 0.4066 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series2 | 1.6530 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series1 | 0.3538 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series1 | 1.3858 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series2 | 0.4416 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series2 | 1.7795 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series1 | 0.3091 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series1 | 1.1264 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series2 | 0.3104 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series2 | 1.1252 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series1 | 2.9196 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series2 | 2.9430 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series1 | 3.5579 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series2 | 3.4649 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series1 | 2.0547 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series2 | 1.9431 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series1 | 2.4022 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series2 | 2.3882 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series1 | 2.7160 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series2 | 2.7684 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series1 | 3.8211 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series2 | 3.7384 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series1 | 2.4448 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series2 | 2.3284 | 0/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series1 | 2.2691 | 0/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series2 | 2.2630 | 0/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series1 | 1.6782 | 0/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series2 | 1.6581 | 0/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series1 | 1.5686 | 0/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series2 | 1.6149 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series1 | 1.2967 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series2 | 1.2876 | 0/6 |
| compare/insert_bulk_10k/lin | sqlite | series1 | 0.9740 | 6/6 |
| compare/insert_bulk_10k/lin | sqlite | series2 | 0.9740 | 5/6 |
| compare/durable_insert_1k/lin | sqlite | series1 | 0.8395 | 6/6 |
| compare/durable_insert_1k/lin | sqlite | series2 | 0.8637 | 6/6 |
| compare/durable_insert_10k/lin | sqlite | series1 | 1.4881 | 0/6 |
| compare/durable_insert_10k/lin | sqlite | series2 | 1.4977 | 0/6 |
| compare/filter_eq/lin | sqlite | series1 | 0.0115 | 6/6 |
| compare/filter_eq/lin | duckdb | series1 | 0.0052 | 6/6 |
| compare/filter_eq/lin | sqlite | series2 | 0.0122 | 6/6 |
| compare/filter_eq/lin | duckdb | series2 | 0.0055 | 6/6 |
| compare/filter_range/lin | sqlite | series1 | 0.0170 | 6/6 |
| compare/filter_range/lin | duckdb | series1 | 0.0038 | 6/6 |
| compare/filter_range/lin | sqlite | series2 | 0.0169 | 6/6 |
| compare/filter_range/lin | duckdb | series2 | 0.0041 | 6/6 |
| compare/agg_count_by/lin | sqlite | series1 | 0.5521 | 6/6 |
| compare/agg_count_by/lin | duckdb | series1 | 0.2876 | 6/6 |
| compare/agg_count_by/lin | sqlite | series2 | 0.5563 | 6/6 |
| compare/agg_count_by/lin | duckdb | series2 | 0.2756 | 6/6 |
| compare/tx_commit_1row_100k/lin | sqlite | series1 | 2.4572 | 0/6 |
| compare/tx_commit_1row_100k/lin | sqlite | series2 | 2.4883 | 0/6 |
| compare/tx_rollback_1row_100k/lin | sqlite | series1 | 2.4934 | 0/6 |
| compare/tx_rollback_1row_100k/lin | sqlite | series2 | 2.4576 | 0/6 |

Source gate: False. Numeric peer wins: 199/456. Whole-process RSS in summary.json includes fixture, oracle and reopen; it is not isolated operation memory.
