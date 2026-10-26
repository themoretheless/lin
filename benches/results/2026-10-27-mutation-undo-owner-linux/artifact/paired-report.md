# Paired native mutation qualification

Two six-round series; target update1k/10k/100k must win6/6 and exceed median absolute A/A. All unchanged controls and SQLite losses retained. This is not eight-peer or Full durable qualification.

| Case | Series | Gain % | A/A % | Positive pairs | SQLite wins |
|---|---|---:|---:|---:|---:|
| compare/update_1row_1k/lin | series1 | 8.969 | 2.574 | 6/6 | 0 |
| compare/update_1row_1k/lin | series2 | 8.422 | 1.264 | 6/6 | 0 |
| compare/update_1row_1k/sqlite | series1 | 1.308 | 2.470 | 4/6 | n/a |
| compare/update_1row_1k/sqlite | series2 | 3.389 | 1.891 | 5/6 | n/a |
| compare/delete_1row_1k/lin | series1 | 1.660 | 3.244 | 3/6 | 0 |
| compare/delete_1row_1k/lin | series2 | -1.019 | 3.170 | 3/6 | 0 |
| compare/delete_1row_1k/sqlite | series1 | 5.003 | 4.946 | 5/6 | n/a |
| compare/delete_1row_1k/sqlite | series2 | 3.631 | 0.548 | 5/6 | n/a |
| compare/update_1row_10k/lin | series1 | 4.987 | 2.109 | 4/6 | 0 |
| compare/update_1row_10k/lin | series2 | 1.884 | 2.493 | 6/6 | 0 |
| compare/update_1row_10k/sqlite | series1 | -3.070 | 6.390 | 3/6 | n/a |
| compare/update_1row_10k/sqlite | series2 | 0.887 | 2.968 | 4/6 | n/a |
| compare/delete_1row_10k/lin | series1 | -0.339 | 3.116 | 3/6 | 0 |
| compare/delete_1row_10k/lin | series2 | 0.775 | 2.240 | 3/6 | 0 |
| compare/delete_1row_10k/sqlite | series1 | -7.473 | 18.361 | 3/6 | n/a |
| compare/delete_1row_10k/sqlite | series2 | 0.985 | 2.357 | 3/6 | n/a |
| compare/update_1row_100k/lin | series1 | 1.604 | 3.722 | 5/6 | 0 |
| compare/update_1row_100k/lin | series2 | 4.845 | 2.180 | 6/6 | 0 |
| compare/update_1row_100k/sqlite | series1 | 4.644 | 8.478 | 4/6 | n/a |
| compare/update_1row_100k/sqlite | series2 | 21.060 | 18.682 | 5/6 | n/a |
| compare/delete_1row_100k/lin | series1 | 2.155 | 2.481 | 5/6 | 0 |
| compare/delete_1row_100k/lin | series2 | -2.013 | 8.997 | 3/6 | 0 |
| compare/delete_1row_100k/sqlite | series1 | 14.828 | 8.607 | 4/6 | n/a |
| compare/delete_1row_100k/sqlite | series2 | 5.299 | 22.803 | 4/6 | n/a |
| compare/tx_commit_1row_1k/lin | series1 | 10.297 | 4.081 | 6/6 | 0 |
| compare/tx_commit_1row_1k/lin | series2 | 8.548 | 1.853 | 6/6 | 0 |
| compare/tx_commit_1row_1k/sqlite | series1 | 1.091 | 3.733 | 4/6 | n/a |
| compare/tx_commit_1row_1k/sqlite | series2 | 0.577 | 2.708 | 4/6 | n/a |
| compare/tx_rollback_1row_1k/lin | series1 | 4.696 | 6.018 | 6/6 | 0 |
| compare/tx_rollback_1row_1k/lin | series2 | 5.431 | 2.496 | 6/6 | 0 |
| compare/tx_rollback_1row_1k/sqlite | series1 | 1.913 | 5.486 | 4/6 | n/a |
| compare/tx_rollback_1row_1k/sqlite | series2 | 1.478 | 1.769 | 4/6 | n/a |
| compare/tx_commit_1row_10k/lin | series1 | 5.894 | 5.267 | 5/6 | 0 |
| compare/tx_commit_1row_10k/lin | series2 | 3.902 | 6.013 | 5/6 | 0 |
| compare/tx_commit_1row_10k/sqlite | series1 | 2.862 | 22.476 | 3/6 | n/a |
| compare/tx_commit_1row_10k/sqlite | series2 | -4.759 | 7.343 | 1/6 | n/a |
| compare/tx_rollback_1row_10k/lin | series1 | 2.919 | 3.857 | 4/6 | 0 |
| compare/tx_rollback_1row_10k/lin | series2 | 0.383 | 3.215 | 3/6 | 0 |
| compare/tx_rollback_1row_10k/sqlite | series1 | 4.613 | 21.871 | 4/6 | n/a |
| compare/tx_rollback_1row_10k/sqlite | series2 | -0.272 | 4.835 | 3/6 | n/a |

Candidate qualified: False. All-eight-peer goal remains open.
