# Paired native mutation qualification

Two six-round series; target update1k/10k/100k must win6/6 and exceed median absolute A/A. All unchanged controls and SQLite losses retained. This is not eight-peer or Full durable qualification.

| Case | Series | Gain % | A/A % | Positive pairs | SQLite wins |
|---|---|---:|---:|---:|---:|
| compare/update_1row_1k/lin | series1 | 1.274 | 2.130 | 5/6 | 0 |
| compare/update_1row_1k/lin | series2 | 3.091 | 1.664 | 6/6 | 0 |
| compare/update_1row_1k/sqlite | series1 | 0.704 | 2.829 | 3/6 | n/a |
| compare/update_1row_1k/sqlite | series2 | 1.781 | 1.749 | 4/6 | n/a |
| compare/delete_1row_1k/lin | series1 | 2.270 | 0.532 | 6/6 | 0 |
| compare/delete_1row_1k/lin | series2 | 3.696 | 1.399 | 5/6 | 0 |
| compare/delete_1row_1k/sqlite | series1 | 1.071 | 2.878 | 3/6 | n/a |
| compare/delete_1row_1k/sqlite | series2 | 1.306 | 0.232 | 5/6 | n/a |
| compare/update_1row_10k/lin | series1 | -0.686 | 2.396 | 3/6 | 0 |
| compare/update_1row_10k/lin | series2 | 0.751 | 2.081 | 4/6 | 0 |
| compare/update_1row_10k/sqlite | series1 | -1.319 | 2.011 | 2/6 | n/a |
| compare/update_1row_10k/sqlite | series2 | 0.284 | 3.951 | 3/6 | n/a |
| compare/delete_1row_10k/lin | series1 | 1.921 | 2.199 | 5/6 | 0 |
| compare/delete_1row_10k/lin | series2 | -0.343 | 2.675 | 3/6 | 0 |
| compare/delete_1row_10k/sqlite | series1 | 4.146 | 2.859 | 4/6 | n/a |
| compare/delete_1row_10k/sqlite | series2 | 2.885 | 3.327 | 4/6 | n/a |
| compare/update_1row_100k/lin | series1 | -1.383 | 1.521 | 2/6 | 0 |
| compare/update_1row_100k/lin | series2 | -1.787 | 0.941 | 2/6 | 0 |
| compare/update_1row_100k/sqlite | series1 | 2.455 | 2.203 | 4/6 | n/a |
| compare/update_1row_100k/sqlite | series2 | -0.615 | 3.281 | 2/6 | n/a |
| compare/delete_1row_100k/lin | series1 | -1.853 | 1.976 | 2/6 | 0 |
| compare/delete_1row_100k/lin | series2 | -0.876 | 2.035 | 3/6 | 0 |
| compare/delete_1row_100k/sqlite | series1 | -0.889 | 1.920 | 3/6 | n/a |
| compare/delete_1row_100k/sqlite | series2 | 4.353 | 3.390 | 6/6 | n/a |
| compare/tx_commit_1row_1k/lin | series1 | -0.207 | 2.223 | 2/6 | 0 |
| compare/tx_commit_1row_1k/lin | series2 | 1.138 | 1.658 | 4/6 | 0 |
| compare/tx_commit_1row_1k/sqlite | series1 | 0.745 | 2.780 | 4/6 | n/a |
| compare/tx_commit_1row_1k/sqlite | series2 | 0.746 | 1.791 | 4/6 | n/a |
| compare/tx_rollback_1row_1k/lin | series1 | 5.985 | 1.558 | 6/6 | 0 |
| compare/tx_rollback_1row_1k/lin | series2 | 7.608 | 2.751 | 6/6 | 0 |
| compare/tx_rollback_1row_1k/sqlite | series1 | 0.828 | 0.619 | 3/6 | n/a |
| compare/tx_rollback_1row_1k/sqlite | series2 | 1.034 | 1.453 | 4/6 | n/a |
| compare/tx_commit_1row_10k/lin | series1 | -0.721 | 4.186 | 3/6 | 0 |
| compare/tx_commit_1row_10k/lin | series2 | -1.143 | 3.682 | 2/6 | 0 |
| compare/tx_commit_1row_10k/sqlite | series1 | -0.167 | 2.131 | 3/6 | n/a |
| compare/tx_commit_1row_10k/sqlite | series2 | 2.015 | 1.256 | 4/6 | n/a |
| compare/tx_rollback_1row_10k/lin | series1 | 4.102 | 1.639 | 5/6 | 0 |
| compare/tx_rollback_1row_10k/lin | series2 | 1.973 | 2.540 | 6/6 | 0 |
| compare/tx_rollback_1row_10k/sqlite | series1 | 0.797 | 2.176 | 3/6 | n/a |
| compare/tx_rollback_1row_10k/sqlite | series2 | 2.026 | 2.081 | 4/6 | n/a |

Candidate qualified: False. All-eight-peer goal remains open.
