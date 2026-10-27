# COW vNext native UPDATE: full-engine paired gate

All 67 cases retained. Each series has six process pairs. Positive gain means candidate faster. Targets require 6/6 positive pairs and median gain above A/A. Controls require gain >= -max(2%, A/A). Full eight-peer objective remains false.

| Case | Series | Target | Gain % | A/A % | Positive | Pass |
|---|---|---|---:|---:|---:|---|
| compare/durable_update_1row_1k/lin | series1 | False | -1.213 | 4.926 | 2/6 | True |
| compare/durable_update_1row_1k/lin | series2 | False | 1.804 | 1.912 | 4/6 | True |
| compare/durable_update_1row_1k/sqlite_delete | series1 | False | -4.768 | 1.618 | 1/6 | False |
| compare/durable_update_1row_1k/sqlite_delete | series2 | False | -1.353 | 3.592 | 2/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series1 | False | 0.673 | 2.370 | 3/6 | True |
| compare/durable_update_1row_1k/sqlite_wal | series2 | False | 0.194 | 1.922 | 3/6 | True |
| compare/durable_delete_1row_1k/lin | series1 | False | -1.772 | 5.142 | 2/6 | True |
| compare/durable_delete_1row_1k/lin | series2 | False | 1.681 | 3.219 | 4/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series1 | False | 0.714 | 5.341 | 4/6 | True |
| compare/durable_delete_1row_1k/sqlite_delete | series2 | False | -3.052 | 5.353 | 1/6 | True |
| compare/durable_delete_1row_1k/sqlite_wal | series1 | False | -3.981 | 0.752 | 0/6 | False |
| compare/durable_delete_1row_1k/sqlite_wal | series2 | False | -0.603 | 7.411 | 3/6 | True |
| compare/durable_commit_1row_1k/lin | series1 | False | -0.089 | 5.846 | 3/6 | True |
| compare/durable_commit_1row_1k/lin | series2 | False | -0.873 | 2.787 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series1 | False | -1.821 | 2.718 | 3/6 | True |
| compare/durable_commit_1row_1k/sqlite_delete | series2 | False | 1.768 | 1.911 | 5/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series1 | False | -3.341 | 3.617 | 1/6 | True |
| compare/durable_commit_1row_1k/sqlite_wal | series2 | False | -1.007 | 3.615 | 2/6 | True |
| compare/durable_rollback_1row_1k/lin | series1 | False | 18.084 | 7.513 | 6/6 | True |
| compare/durable_rollback_1row_1k/lin | series2 | False | 15.091 | 13.185 | 5/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series1 | False | 0.329 | 2.888 | 4/6 | True |
| compare/durable_rollback_1row_1k/sqlite_delete | series2 | False | 2.353 | 2.624 | 5/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series1 | False | 1.005 | 1.276 | 5/6 | True |
| compare/durable_rollback_1row_1k/sqlite_wal | series2 | False | 0.794 | 0.872 | 4/6 | True |
| compare/durable_update_1row_10k/lin | series1 | False | 5.215 | 4.541 | 5/6 | True |
| compare/durable_update_1row_10k/lin | series2 | False | 4.044 | 2.873 | 4/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series1 | False | 1.307 | 7.868 | 5/6 | True |
| compare/durable_update_1row_10k/sqlite_delete | series2 | False | 0.505 | 2.464 | 3/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series1 | False | -1.029 | 3.220 | 3/6 | True |
| compare/durable_update_1row_10k/sqlite_wal | series2 | False | 0.176 | 3.714 | 3/6 | True |
| compare/durable_delete_1row_10k/lin | series1 | False | -0.420 | 5.028 | 3/6 | True |
| compare/durable_delete_1row_10k/lin | series2 | False | -1.790 | 4.492 | 3/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series1 | False | 1.054 | 2.863 | 3/6 | True |
| compare/durable_delete_1row_10k/sqlite_delete | series2 | False | 1.695 | 3.431 | 4/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series1 | False | 1.223 | 7.287 | 4/6 | True |
| compare/durable_delete_1row_10k/sqlite_wal | series2 | False | -1.547 | 7.267 | 3/6 | True |
| compare/durable_commit_1row_10k/lin | series1 | False | -1.825 | 2.807 | 3/6 | True |
| compare/durable_commit_1row_10k/lin | series2 | False | 6.266 | 5.940 | 5/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series1 | False | 1.946 | 1.776 | 4/6 | True |
| compare/durable_commit_1row_10k/sqlite_delete | series2 | False | 0.215 | 2.447 | 4/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series1 | False | 0.791 | 7.191 | 4/6 | True |
| compare/durable_commit_1row_10k/sqlite_wal | series2 | False | -3.021 | 2.599 | 1/6 | False |
| compare/durable_rollback_1row_10k/lin | series1 | False | 13.291 | 1.213 | 6/6 | True |
| compare/durable_rollback_1row_10k/lin | series2 | False | 13.704 | 1.941 | 6/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series1 | False | -0.149 | 4.328 | 3/6 | True |
| compare/durable_rollback_1row_10k/sqlite_delete | series2 | False | 1.612 | 3.178 | 4/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series1 | False | 0.347 | 2.002 | 3/6 | True |
| compare/durable_rollback_1row_10k/sqlite_wal | series2 | False | 1.047 | 1.683 | 3/6 | True |
| compare/update_1row_1k/lin | series1 | True | 21.126 | 1.092 | 6/6 | True |
| compare/update_1row_1k/lin | series2 | True | 20.708 | 0.488 | 6/6 | True |
| compare/update_1row_1k/sqlite | series1 | False | -3.320 | 2.210 | 1/6 | False |
| compare/update_1row_1k/sqlite | series2 | False | -0.586 | 1.655 | 2/6 | True |
| compare/delete_1row_1k/lin | series1 | False | -2.841 | 3.567 | 0/6 | True |
| compare/delete_1row_1k/lin | series2 | False | 3.227 | 3.088 | 5/6 | True |
| compare/delete_1row_1k/sqlite | series1 | False | -0.406 | 1.631 | 3/6 | True |
| compare/delete_1row_1k/sqlite | series2 | False | 0.259 | 2.686 | 3/6 | True |
| compare/update_1row_10k/lin | series1 | True | 16.253 | 0.799 | 6/6 | True |
| compare/update_1row_10k/lin | series2 | True | 15.599 | 2.986 | 6/6 | True |
| compare/update_1row_10k/sqlite | series1 | False | -1.860 | 4.691 | 3/6 | True |
| compare/update_1row_10k/sqlite | series2 | False | 0.964 | 3.558 | 3/6 | True |
| compare/delete_1row_10k/lin | series1 | False | 1.054 | 1.702 | 4/6 | True |
| compare/delete_1row_10k/lin | series2 | False | 2.116 | 0.944 | 5/6 | True |
| compare/delete_1row_10k/sqlite | series1 | False | -2.043 | 2.373 | 1/6 | True |
| compare/delete_1row_10k/sqlite | series2 | False | -3.623 | 1.872 | 1/6 | False |
| compare/update_1row_100k/lin | series1 | True | 21.570 | 3.088 | 6/6 | True |
| compare/update_1row_100k/lin | series2 | True | 19.810 | 4.944 | 6/6 | True |
| compare/update_1row_100k/sqlite | series1 | False | -5.859 | 10.961 | 2/6 | True |
| compare/update_1row_100k/sqlite | series2 | False | 3.548 | 15.152 | 3/6 | True |
| compare/delete_1row_100k/lin | series1 | False | 10.070 | 2.551 | 6/6 | True |
| compare/delete_1row_100k/lin | series2 | False | 7.885 | 1.807 | 6/6 | True |
| compare/delete_1row_100k/sqlite | series1 | False | 8.308 | 2.372 | 4/6 | True |
| compare/delete_1row_100k/sqlite | series2 | False | 13.729 | 8.970 | 4/6 | True |
| compare/tx_commit_1row_1k/lin | series1 | False | 20.169 | 2.137 | 6/6 | True |
| compare/tx_commit_1row_1k/lin | series2 | False | 19.874 | 1.318 | 6/6 | True |
| compare/tx_commit_1row_1k/sqlite | series1 | False | -0.464 | 2.394 | 3/6 | True |
| compare/tx_commit_1row_1k/sqlite | series2 | False | 0.319 | 3.089 | 3/6 | True |
| compare/tx_rollback_1row_1k/lin | series1 | False | 21.158 | 2.464 | 6/6 | True |
| compare/tx_rollback_1row_1k/lin | series2 | False | 20.180 | 1.009 | 6/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series1 | False | -0.204 | 1.995 | 3/6 | True |
| compare/tx_rollback_1row_1k/sqlite | series2 | False | 0.040 | 2.224 | 3/6 | True |
| compare/tx_commit_1row_10k/lin | series1 | False | 14.408 | 2.753 | 6/6 | True |
| compare/tx_commit_1row_10k/lin | series2 | False | 13.294 | 1.061 | 6/6 | True |
| compare/tx_commit_1row_10k/sqlite | series1 | False | 0.113 | 4.906 | 3/6 | True |
| compare/tx_commit_1row_10k/sqlite | series2 | False | -1.299 | 5.165 | 1/6 | True |
| compare/tx_rollback_1row_10k/lin | series1 | False | 13.914 | 1.789 | 6/6 | True |
| compare/tx_rollback_1row_10k/lin | series2 | False | 12.695 | 0.893 | 6/6 | True |
| compare/tx_rollback_1row_10k/sqlite | series1 | False | -2.141 | 0.926 | 2/6 | False |
| compare/tx_rollback_1row_10k/sqlite | series2 | False | -2.117 | 0.596 | 2/6 | False |
| compare/insert_bulk_1k/lin | series1 | False | -0.757 | 0.925 | 2/6 | True |
| compare/insert_bulk_1k/lin | series2 | False | 0.902 | 0.303 | 5/6 | True |
| compare/insert_bulk_1k/sqlite | series1 | False | -1.413 | 1.986 | 1/6 | True |
| compare/insert_bulk_1k/sqlite | series2 | False | -2.676 | 0.414 | 1/6 | False |
| compare/insert_bulk_10k/lin | series1 | False | 0.478 | 1.019 | 4/6 | True |
| compare/insert_bulk_10k/lin | series2 | False | 0.394 | 1.320 | 4/6 | True |
| compare/insert_bulk_10k/sqlite | series1 | False | -0.007 | 0.468 | 3/6 | True |
| compare/insert_bulk_10k/sqlite | series2 | False | 0.036 | 0.719 | 3/6 | True |
| compare/durable_insert_1k/lin | series1 | False | 1.107 | 3.566 | 3/6 | True |
| compare/durable_insert_1k/lin | series2 | False | 0.712 | 2.287 | 6/6 | True |
| compare/durable_insert_1k/sqlite | series1 | False | 0.479 | 2.093 | 4/6 | True |
| compare/durable_insert_1k/sqlite | series2 | False | -0.417 | 0.532 | 2/6 | True |
| compare/durable_insert_10k/lin | series1 | False | -0.682 | 58.798 | 3/6 | True |
| compare/durable_insert_10k/lin | series2 | False | -1.554 | 63.302 | 1/6 | True |
| compare/durable_insert_10k/sqlite | series1 | False | 0.057 | 0.277 | 4/6 | True |
| compare/durable_insert_10k/sqlite | series2 | False | -0.415 | 0.736 | 2/6 | True |
| compare/filter_eq/lin | series1 | False | 4.440 | 3.224 | 5/6 | True |
| compare/filter_eq/lin | series2 | False | 5.801 | 2.686 | 6/6 | True |
| compare/filter_eq/sqlite | series1 | False | 0.265 | 0.564 | 4/6 | True |
| compare/filter_eq/sqlite | series2 | False | 0.320 | 0.798 | 4/6 | True |
| compare/filter_eq/duckdb | series1 | False | 2.730 | 9.038 | 3/6 | True |
| compare/filter_eq/duckdb | series2 | False | -3.046 | 9.786 | 1/6 | True |
| compare/filter_range/lin | series1 | False | 7.159 | 1.416 | 6/6 | True |
| compare/filter_range/lin | series2 | False | 6.648 | 1.393 | 6/6 | True |
| compare/filter_range/sqlite | series1 | False | 1.528 | 1.730 | 4/6 | True |
| compare/filter_range/sqlite | series2 | False | -0.891 | 0.669 | 2/6 | True |
| compare/filter_range/duckdb | series1 | False | 3.340 | 1.504 | 4/6 | True |
| compare/filter_range/duckdb | series2 | False | -0.418 | 2.457 | 3/6 | True |
| compare/agg_count_by/lin | series1 | False | 1.349 | 3.598 | 4/6 | True |
| compare/agg_count_by/lin | series2 | False | -3.226 | 3.390 | 0/6 | True |
| compare/agg_count_by/sqlite | series1 | False | 0.471 | 1.560 | 4/6 | True |
| compare/agg_count_by/sqlite | series2 | False | 0.321 | 1.019 | 3/6 | True |
| compare/agg_count_by/duckdb | series1 | False | 0.544 | 7.905 | 3/6 | True |
| compare/agg_count_by/duckdb | series2 | False | -2.436 | 7.700 | 2/6 | True |
| compare/wal_scalar_full_update/lin | series1 | False | -3.321 | 4.507 | 1/6 | True |
| compare/wal_scalar_full_update/lin | series2 | False | -2.887 | 2.068 | 1/6 | False |
| compare/wal_scalar_normal_update/lin | series1 | False | -4.223 | 1.206 | 1/6 | False |
| compare/wal_scalar_normal_update/lin | series2 | False | -2.247 | 1.026 | 2/6 | False |
| compare/tx_commit_1row_100k/lin | series1 | False | 23.849 | 5.031 | 6/6 | True |
| compare/tx_commit_1row_100k/lin | series2 | False | 24.481 | 10.016 | 6/6 | True |
| compare/tx_commit_1row_100k/sqlite | series1 | False | -0.553 | 9.747 | 3/6 | True |
| compare/tx_commit_1row_100k/sqlite | series2 | False | 12.203 | 4.011 | 4/6 | True |
| compare/tx_rollback_1row_100k/lin | series1 | False | 17.121 | 4.181 | 6/6 | True |
| compare/tx_rollback_1row_100k/lin | series2 | False | 17.078 | 0.834 | 6/6 | True |
| compare/tx_rollback_1row_100k/sqlite | series1 | False | 2.820 | 8.314 | 4/6 | True |
| compare/tx_rollback_1row_100k/sqlite | series2 | False | -0.332 | 8.389 | 3/6 | True |

| Lin case | Peer | Series | Lin/peer | Lin wins |
|---|---|---|---:|---:|
| compare/durable_update_1row_1k/lin | sqlite_delete | series1 | 0.4232 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series1 | 1.1632 | 0/6 |
| compare/durable_update_1row_1k/lin | sqlite_delete | series2 | 0.4015 | 6/6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | series2 | 1.1500 | 0/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series1 | 0.4323 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series1 | 1.1365 | 0/6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | series2 | 0.4257 | 6/6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | series2 | 1.1053 | 0/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series1 | 0.4240 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series1 | 1.1165 | 0/6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | series2 | 0.4494 | 6/6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | series2 | 1.1216 | 0/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series1 | 0.1903 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series1 | 1.1405 | 2/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | series2 | 0.2043 | 6/6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | series2 | 1.2691 | 1/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series1 | 0.6131 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series1 | 1.5641 | 0/6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | series2 | 0.6177 | 6/6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | series2 | 1.5395 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series1 | 0.6186 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series1 | 1.6516 | 0/6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | series2 | 0.6076 | 6/6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | series2 | 1.6138 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series1 | 0.6341 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series1 | 1.5614 | 0/6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | series2 | 0.6323 | 6/6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | series2 | 1.4852 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series1 | 0.2840 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series1 | 1.6563 | 0/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | series2 | 0.2915 | 6/6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | series2 | 1.6750 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series1 | 1.3607 | 0/6 |
| compare/update_1row_1k/lin | sqlite | series2 | 1.3778 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series1 | 2.0921 | 0/6 |
| compare/delete_1row_1k/lin | sqlite | series2 | 2.0805 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series1 | 1.8952 | 0/6 |
| compare/update_1row_10k/lin | sqlite | series2 | 2.0587 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series1 | 2.8676 | 0/6 |
| compare/delete_1row_10k/lin | sqlite | series2 | 2.4546 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series1 | 1.5360 | 0/6 |
| compare/update_1row_100k/lin | sqlite | series2 | 1.5015 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series1 | 1.8755 | 0/6 |
| compare/delete_1row_100k/lin | sqlite | series2 | 2.5034 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series1 | 1.1459 | 0/6 |
| compare/tx_commit_1row_1k/lin | sqlite | series2 | 1.1023 | 0/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series1 | 1.4748 | 0/6 |
| compare/tx_rollback_1row_1k/lin | sqlite | series2 | 1.5108 | 0/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series1 | 1.4901 | 0/6 |
| compare/tx_commit_1row_10k/lin | sqlite | series2 | 1.7849 | 0/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series1 | 1.8807 | 0/6 |
| compare/tx_rollback_1row_10k/lin | sqlite | series2 | 2.1788 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series1 | 1.1731 | 0/6 |
| compare/insert_bulk_1k/lin | sqlite | series2 | 1.1333 | 0/6 |
| compare/insert_bulk_10k/lin | sqlite | series1 | 0.9911 | 3/6 |
| compare/insert_bulk_10k/lin | sqlite | series2 | 0.9526 | 6/6 |
| compare/durable_insert_1k/lin | sqlite | series1 | 0.8795 | 6/6 |
| compare/durable_insert_1k/lin | sqlite | series2 | 0.8474 | 6/6 |
| compare/durable_insert_10k/lin | sqlite | series1 | 0.9672 | 4/6 |
| compare/durable_insert_10k/lin | sqlite | series2 | 0.9513 | 6/6 |
| compare/filter_eq/lin | sqlite | series1 | 0.0099 | 6/6 |
| compare/filter_eq/lin | duckdb | series1 | 0.0051 | 6/6 |
| compare/filter_eq/lin | sqlite | series2 | 0.0096 | 6/6 |
| compare/filter_eq/lin | duckdb | series2 | 0.0052 | 6/6 |
| compare/filter_range/lin | sqlite | series1 | 0.0139 | 6/6 |
| compare/filter_range/lin | duckdb | series1 | 0.0045 | 6/6 |
| compare/filter_range/lin | sqlite | series2 | 0.0141 | 6/6 |
| compare/filter_range/lin | duckdb | series2 | 0.0045 | 6/6 |
| compare/agg_count_by/lin | sqlite | series1 | 0.4182 | 6/6 |
| compare/agg_count_by/lin | duckdb | series1 | 0.2765 | 6/6 |
| compare/agg_count_by/lin | sqlite | series2 | 0.4204 | 5/6 |
| compare/agg_count_by/lin | duckdb | series2 | 0.2978 | 6/6 |
| compare/tx_commit_1row_100k/lin | sqlite | series1 | 1.1849 | 0/6 |
| compare/tx_commit_1row_100k/lin | sqlite | series2 | 1.3906 | 1/6 |
| compare/tx_rollback_1row_100k/lin | sqlite | series1 | 1.5142 | 0/6 |
| compare/tx_rollback_1row_100k/lin | sqlite | series2 | 1.5842 | 0/6 |

Source gate: False. Numeric peer wins: 202/456. Whole-process RSS in summary.json includes fixture, oracle and reopen; it is not isolated operation memory.
