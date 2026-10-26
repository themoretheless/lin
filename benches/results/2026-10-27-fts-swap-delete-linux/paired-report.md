# Paired native mutation qualification

Two six-round series; target delete1k/10k/100k must win6/6 and exceed median absolute A/A. All unchanged controls and SQLite losses retained. This is not eight-peer or Full durable qualification.

| Case | Series | Gain % | A/A % | Positive pairs | SQLite wins |
|---|---|---:|---:|---:|---:|
| compare/update_1row_1k/lin | series1 | -0.320 | 2.505 | 3/6 | 0 |
| compare/update_1row_1k/lin | series2 | 2.953 | 3.147 | 4/6 | 0 |
| compare/update_1row_1k/sqlite | series1 | -0.249 | 2.875 | 2/6 | n/a |
| compare/update_1row_1k/sqlite | series2 | 1.230 | 1.580 | 4/6 | n/a |
| compare/delete_1row_1k/lin | series1 | 7.940 | 4.683 | 6/6 | 0 |
| compare/delete_1row_1k/lin | series2 | 3.417 | 2.061 | 6/6 | 0 |
| compare/delete_1row_1k/sqlite | series1 | 1.057 | 4.638 | 4/6 | n/a |
| compare/delete_1row_1k/sqlite | series2 | 1.286 | 4.670 | 3/6 | n/a |
| compare/update_1row_10k/lin | series1 | -2.562 | 1.111 | 1/6 | 0 |
| compare/update_1row_10k/lin | series2 | -0.783 | 2.498 | 3/6 | 0 |
| compare/update_1row_10k/sqlite | series1 | 2.088 | 5.498 | 4/6 | n/a |
| compare/update_1row_10k/sqlite | series2 | 6.241 | 8.273 | 5/6 | n/a |
| compare/delete_1row_10k/lin | series1 | -1.079 | 1.943 | 2/6 | 0 |
| compare/delete_1row_10k/lin | series2 | 1.809 | 2.624 | 5/6 | 0 |
| compare/delete_1row_10k/sqlite | series1 | -3.349 | 2.949 | 2/6 | n/a |
| compare/delete_1row_10k/sqlite | series2 | -4.618 | 8.251 | 3/6 | n/a |
| compare/update_1row_100k/lin | series1 | -1.357 | 3.302 | 3/6 | 0 |
| compare/update_1row_100k/lin | series2 | 0.596 | 1.477 | 4/6 | 0 |
| compare/update_1row_100k/sqlite | series1 | 2.690 | 9.989 | 4/6 | n/a |
| compare/update_1row_100k/sqlite | series2 | 10.373 | 13.137 | 4/6 | n/a |
| compare/delete_1row_100k/lin | series1 | -0.755 | 6.261 | 3/6 | 0 |
| compare/delete_1row_100k/lin | series2 | 1.323 | 2.646 | 5/6 | 0 |
| compare/delete_1row_100k/sqlite | series1 | -1.855 | 10.465 | 2/6 | n/a |
| compare/delete_1row_100k/sqlite | series2 | 4.732 | 11.443 | 4/6 | n/a |
| compare/tx_commit_1row_1k/lin | series1 | -1.885 | 7.465 | 2/6 | 0 |
| compare/tx_commit_1row_1k/lin | series2 | -1.546 | 3.524 | 2/6 | 0 |
| compare/tx_commit_1row_1k/sqlite | series1 | 2.314 | 3.344 | 5/6 | n/a |
| compare/tx_commit_1row_1k/sqlite | series2 | 1.330 | 2.970 | 4/6 | n/a |
| compare/tx_rollback_1row_1k/lin | series1 | -2.561 | 4.409 | 2/6 | 0 |
| compare/tx_rollback_1row_1k/lin | series2 | 0.352 | 4.361 | 3/6 | 0 |
| compare/tx_rollback_1row_1k/sqlite | series1 | -0.267 | 2.636 | 3/6 | n/a |
| compare/tx_rollback_1row_1k/sqlite | series2 | -1.979 | 1.333 | 2/6 | n/a |
| compare/tx_commit_1row_10k/lin | series1 | -3.071 | 4.178 | 1/6 | 0 |
| compare/tx_commit_1row_10k/lin | series2 | -4.190 | 3.330 | 1/6 | 0 |
| compare/tx_commit_1row_10k/sqlite | series1 | 9.961 | 3.977 | 5/6 | n/a |
| compare/tx_commit_1row_10k/sqlite | series2 | 2.310 | 4.963 | 3/6 | n/a |
| compare/tx_rollback_1row_10k/lin | series1 | -0.199 | 4.645 | 3/6 | 0 |
| compare/tx_rollback_1row_10k/lin | series2 | -3.984 | 3.588 | 1/6 | 0 |
| compare/tx_rollback_1row_10k/sqlite | series1 | -4.486 | 11.554 | 2/6 | n/a |
| compare/tx_rollback_1row_10k/sqlite | series2 | -9.597 | 6.047 | 1/6 | n/a |

Candidate qualified: False. All-eight-peer goal remains open.
