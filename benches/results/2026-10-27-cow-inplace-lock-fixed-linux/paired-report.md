# Paired native mutation qualification

Two six-round series; target update1k/10k/100k must win6/6 and exceed median absolute A/A. All unchanged controls and SQLite losses retained. This is not eight-peer or Full durable qualification.

| Case | Series | Gain % | A/A % | Positive pairs | SQLite wins |
|---|---|---:|---:|---:|---:|
| compare/update_1row_1k/lin | series1 | 54.834 | 9.116 | 6/6 | 4 |
| compare/update_1row_1k/lin | series2 | 58.321 | 7.310 | 6/6 | 4 |
| compare/update_1row_1k/sqlite | series1 | -12.415 | 6.727 | 0/6 | n/a |
| compare/update_1row_1k/sqlite | series2 | -0.432 | 3.475 | 3/6 | n/a |
| compare/delete_1row_1k/lin | series1 | -0.555 | 12.124 | 3/6 | 0 |
| compare/delete_1row_1k/lin | series2 | 8.775 | 5.280 | 6/6 | 0 |
| compare/delete_1row_1k/sqlite | series1 | -1.063 | 6.868 | 1/6 | n/a |
| compare/delete_1row_1k/sqlite | series2 | 1.244 | 2.800 | 4/6 | n/a |
| compare/update_1row_10k/lin | series1 | 55.459 | 5.471 | 6/6 | 4 |
| compare/update_1row_10k/lin | series2 | 57.838 | 5.149 | 6/6 | 6 |
| compare/update_1row_10k/sqlite | series1 | 0.909 | 9.751 | 4/6 | n/a |
| compare/update_1row_10k/sqlite | series2 | -1.508 | 14.623 | 3/6 | n/a |
| compare/delete_1row_10k/lin | series1 | 4.697 | 2.776 | 6/6 | 0 |
| compare/delete_1row_10k/lin | series2 | 1.290 | 4.428 | 4/6 | 0 |
| compare/delete_1row_10k/sqlite | series1 | -3.858 | 4.684 | 3/6 | n/a |
| compare/delete_1row_10k/sqlite | series2 | -7.208 | 16.681 | 2/6 | n/a |
| compare/update_1row_100k/lin | series1 | 55.548 | 5.810 | 6/6 | 5 |
| compare/update_1row_100k/lin | series2 | 54.012 | 3.522 | 6/6 | 5 |
| compare/update_1row_100k/sqlite | series1 | -6.792 | 10.527 | 2/6 | n/a |
| compare/update_1row_100k/sqlite | series2 | 2.520 | 15.808 | 3/6 | n/a |
| compare/delete_1row_100k/lin | series1 | 8.883 | 3.920 | 5/6 | 0 |
| compare/delete_1row_100k/lin | series2 | 6.949 | 3.738 | 4/6 | 0 |
| compare/delete_1row_100k/sqlite | series1 | 6.403 | 15.170 | 4/6 | n/a |
| compare/delete_1row_100k/sqlite | series2 | 6.349 | 11.628 | 5/6 | n/a |
| compare/tx_commit_1row_1k/lin | series1 | 70.272 | 8.832 | 6/6 | 6 |
| compare/tx_commit_1row_1k/lin | series2 | 66.947 | 5.750 | 6/6 | 6 |
| compare/tx_commit_1row_1k/sqlite | series1 | -3.357 | 3.112 | 2/6 | n/a |
| compare/tx_commit_1row_1k/sqlite | series2 | -1.146 | 3.180 | 3/6 | n/a |
| compare/tx_rollback_1row_1k/lin | series1 | 58.921 | 7.156 | 6/6 | 0 |
| compare/tx_rollback_1row_1k/lin | series2 | 52.217 | 5.672 | 6/6 | 1 |
| compare/tx_rollback_1row_1k/sqlite | series1 | -2.104 | 4.619 | 1/6 | n/a |
| compare/tx_rollback_1row_1k/sqlite | series2 | -3.497 | 2.638 | 2/6 | n/a |
| compare/tx_commit_1row_10k/lin | series1 | 67.318 | 3.793 | 6/6 | 6 |
| compare/tx_commit_1row_10k/lin | series2 | 65.338 | 7.186 | 6/6 | 6 |
| compare/tx_commit_1row_10k/sqlite | series1 | 3.415 | 5.091 | 4/6 | n/a |
| compare/tx_commit_1row_10k/sqlite | series2 | -5.330 | 10.430 | 1/6 | n/a |
| compare/tx_rollback_1row_10k/lin | series1 | 54.863 | 3.795 | 6/6 | 0 |
| compare/tx_rollback_1row_10k/lin | series2 | 54.332 | 3.844 | 6/6 | 0 |
| compare/tx_rollback_1row_10k/sqlite | series1 | -8.021 | 6.707 | 2/6 | n/a |
| compare/tx_rollback_1row_10k/sqlite | series2 | -4.389 | 9.425 | 2/6 | n/a |

Candidate qualified: False. All-eight-peer goal remains open.
