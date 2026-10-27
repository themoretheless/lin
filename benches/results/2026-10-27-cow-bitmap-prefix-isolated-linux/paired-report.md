# Paired native mutation qualification

Two six-round series; target update1k/10k/100k must win6/6 and exceed median absolute A/A. All unchanged controls and SQLite losses retained. This is not eight-peer or Full durable qualification.

| Case | Series | Gain % | A/A % | Positive pairs | SQLite wins |
|---|---|---:|---:|---:|---:|
| compare/update_1row_1k/sqlite | series1 | 0.428 | 4.492 | 3/6 | n/a |
| compare/update_1row_1k/sqlite | series2 | 3.356 | 4.405 | 6/6 | n/a |
| compare/delete_1row_1k/lin | series1 | 10.750 | 2.053 | 6/6 | 0 |
| compare/delete_1row_1k/lin | series2 | 13.424 | 6.257 | 6/6 | 0 |
| compare/delete_1row_1k/sqlite | series1 | -0.810 | 7.659 | 3/6 | n/a |
| compare/delete_1row_1k/sqlite | series2 | -1.074 | 3.102 | 2/6 | n/a |
| compare/update_1row_10k/lin | series1 | 45.817 | 3.623 | 6/6 | 0 |
| compare/update_1row_10k/lin | series2 | 46.016 | 2.889 | 6/6 | 1 |
| compare/update_1row_10k/sqlite | series1 | 4.480 | 10.549 | 4/6 | n/a |
| compare/update_1row_10k/sqlite | series2 | 0.054 | 8.317 | 3/6 | n/a |
| compare/delete_1row_10k/lin | series1 | 9.526 | 4.284 | 6/6 | 0 |
| compare/delete_1row_10k/lin | series2 | 9.695 | 4.412 | 6/6 | 0 |
| compare/delete_1row_10k/sqlite | series1 | 4.828 | 16.909 | 4/6 | n/a |
| compare/delete_1row_10k/sqlite | series2 | 1.135 | 9.805 | 4/6 | n/a |
| compare/update_1row_100k/lin | series1 | 48.150 | 0.506 | 6/6 | 3 |
| compare/update_1row_100k/lin | series2 | 49.530 | 2.046 | 6/6 | 4 |
| compare/update_1row_100k/sqlite | series1 | 10.377 | 14.926 | 4/6 | n/a |
| compare/update_1row_100k/sqlite | series2 | 7.018 | 8.228 | 5/6 | n/a |
| compare/delete_1row_100k/lin | series1 | 10.053 | 3.597 | 6/6 | 0 |
| compare/delete_1row_100k/lin | series2 | 12.346 | 1.957 | 6/6 | 0 |
| compare/delete_1row_100k/sqlite | series1 | -7.030 | 9.871 | 3/6 | n/a |
| compare/delete_1row_100k/sqlite | series2 | -1.124 | 12.804 | 3/6 | n/a |
| compare/tx_commit_1row_1k/lin | series1 | 71.437 | 8.766 | 6/6 | 5 |
| compare/tx_commit_1row_1k/lin | series2 | 70.221 | 9.165 | 6/6 | 6 |
| compare/tx_commit_1row_1k/sqlite | series1 | 0.006 | 4.605 | 3/6 | n/a |
| compare/tx_commit_1row_1k/sqlite | series2 | -1.694 | 6.838 | 2/6 | n/a |
| compare/tx_rollback_1row_1k/lin | series1 | 54.395 | 5.375 | 6/6 | 5 |
| compare/tx_rollback_1row_1k/lin | series2 | 52.720 | 2.128 | 6/6 | 1 |
| compare/tx_rollback_1row_1k/sqlite | series1 | -2.379 | 3.118 | 2/6 | n/a |
| compare/tx_rollback_1row_1k/sqlite | series2 | 6.270 | 8.473 | 5/6 | n/a |
| compare/tx_commit_1row_10k/lin | series1 | 47.184 | 1.237 | 6/6 | 0 |
| compare/tx_commit_1row_10k/lin | series2 | 48.024 | 1.326 | 6/6 | 3 |
| compare/tx_commit_1row_10k/sqlite | series1 | 2.726 | 5.200 | 3/6 | n/a |
| compare/tx_commit_1row_10k/sqlite | series2 | 3.340 | 5.528 | 3/6 | n/a |
| compare/tx_rollback_1row_10k/lin | series1 | 31.510 | 3.862 | 6/6 | 0 |
| compare/tx_rollback_1row_10k/lin | series2 | 35.124 | 2.009 | 6/6 | 0 |
| compare/tx_rollback_1row_10k/sqlite | series1 | -17.601 | 20.946 | 1/6 | n/a |
| compare/tx_rollback_1row_10k/sqlite | series2 | -5.916 | 7.562 | 2/6 | n/a |
| compare/update_1row_1k/lin | series1 | 68.597 | 5.242 | 6/6 | 6 |
| compare/update_1row_1k/lin | series2 | 70.044 | 8.623 | 6/6 | 5 |

Candidate qualified: True. All-eight-peer goal remains open.
