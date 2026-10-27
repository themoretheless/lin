# Paired native mutation qualification

Two six-round series; target update1k/10k/100k must win6/6 and exceed median absolute A/A. All unchanged controls and SQLite losses retained. This is not eight-peer or Full durable qualification.

| Case | Series | Gain % | A/A % | Positive pairs | SQLite wins |
|---|---|---:|---:|---:|---:|
| compare/update_1row_1k/lin | series1 | 22.400 | 10.625 | 6/6 | 0 |
| compare/update_1row_1k/lin | series2 | 24.024 | 17.723 | 4/6 | 0 |
| compare/update_1row_1k/sqlite | series1 | 2.614 | 3.742 | 4/6 | n/a |
| compare/update_1row_1k/sqlite | series2 | 0.559 | 2.373 | 4/6 | n/a |
| compare/delete_1row_1k/lin | series1 | 15.007 | 7.125 | 6/6 | 0 |
| compare/delete_1row_1k/lin | series2 | 5.687 | 4.839 | 5/6 | 0 |
| compare/delete_1row_1k/sqlite | series1 | 1.205 | 5.163 | 3/6 | n/a |
| compare/delete_1row_1k/sqlite | series2 | 3.529 | 1.995 | 4/6 | n/a |
| compare/update_1row_10k/lin | series1 | 10.358 | 3.969 | 6/6 | 0 |
| compare/update_1row_10k/lin | series2 | 8.853 | 6.903 | 5/6 | 0 |
| compare/update_1row_10k/sqlite | series1 | -6.652 | 9.022 | 2/6 | n/a |
| compare/update_1row_10k/sqlite | series2 | 7.665 | 24.228 | 4/6 | n/a |
| compare/delete_1row_10k/lin | series1 | 3.895 | 3.887 | 5/6 | 0 |
| compare/delete_1row_10k/lin | series2 | 3.357 | 7.151 | 4/6 | 0 |
| compare/delete_1row_10k/sqlite | series1 | 3.932 | 9.677 | 4/6 | n/a |
| compare/delete_1row_10k/sqlite | series2 | -10.729 | 45.475 | 2/6 | n/a |
| compare/update_1row_100k/lin | series1 | 6.654 | 5.481 | 5/6 | 0 |
| compare/update_1row_100k/lin | series2 | 5.713 | 3.152 | 5/6 | 0 |
| compare/update_1row_100k/sqlite | series1 | 2.602 | 9.808 | 5/6 | n/a |
| compare/update_1row_100k/sqlite | series2 | 10.098 | 12.902 | 4/6 | n/a |
| compare/delete_1row_100k/lin | series1 | 5.408 | 6.164 | 6/6 | 0 |
| compare/delete_1row_100k/lin | series2 | 3.794 | 3.301 | 5/6 | 0 |
| compare/delete_1row_100k/sqlite | series1 | -7.767 | 9.803 | 3/6 | n/a |
| compare/delete_1row_100k/sqlite | series2 | -5.446 | 15.479 | 2/6 | n/a |
| compare/tx_commit_1row_1k/lin | series1 | -2.150 | 2.951 | 3/6 | 0 |
| compare/tx_commit_1row_1k/lin | series2 | 14.117 | 4.273 | 4/6 | 0 |
| compare/tx_commit_1row_1k/sqlite | series1 | -5.454 | 6.148 | 0/6 | n/a |
| compare/tx_commit_1row_1k/sqlite | series2 | 1.923 | 4.227 | 4/6 | n/a |
| compare/tx_rollback_1row_1k/lin | series1 | 0.036 | 5.186 | 3/6 | 0 |
| compare/tx_rollback_1row_1k/lin | series2 | 22.406 | 7.094 | 5/6 | 0 |
| compare/tx_rollback_1row_1k/sqlite | series1 | -11.733 | 10.812 | 1/6 | n/a |
| compare/tx_rollback_1row_1k/sqlite | series2 | -7.235 | 13.864 | 2/6 | n/a |
| compare/tx_commit_1row_10k/lin | series1 | 11.854 | 4.319 | 5/6 | 0 |
| compare/tx_commit_1row_10k/lin | series2 | 12.684 | 3.230 | 6/6 | 0 |
| compare/tx_commit_1row_10k/sqlite | series1 | -4.044 | 11.387 | 3/6 | n/a |
| compare/tx_commit_1row_10k/sqlite | series2 | -0.845 | 20.667 | 3/6 | n/a |
| compare/tx_rollback_1row_10k/lin | series1 | 14.979 | 2.818 | 5/6 | 0 |
| compare/tx_rollback_1row_10k/lin | series2 | 10.934 | 3.633 | 6/6 | 0 |
| compare/tx_rollback_1row_10k/sqlite | series1 | -21.158 | 32.993 | 2/6 | n/a |
| compare/tx_rollback_1row_10k/sqlite | series2 | 1.149 | 8.831 | 3/6 | n/a |

Candidate qualified: False. All-eight-peer goal remains open.
