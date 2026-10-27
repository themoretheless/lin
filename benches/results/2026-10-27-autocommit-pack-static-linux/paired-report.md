# Paired native mutation qualification

Two six-round series; target update1k/10k/100k must win6/6 and exceed median absolute A/A. All unchanged controls and SQLite losses retained. This is not eight-peer or Full durable qualification.

| Case | Series | Gain % | A/A % | Positive pairs | SQLite wins |
|---|---|---:|---:|---:|---:|
| compare/update_1row_1k/sqlite | series1 | 1.821 | 0.913 | 6/6 | n/a |
| compare/update_1row_1k/sqlite | series2 | 0.766 | 3.039 | 3/6 | n/a |
| compare/delete_1row_1k/lin | series1 | 1.631 | 3.349 | 5/6 | 0 |
| compare/delete_1row_1k/lin | series2 | 1.544 | 1.911 | 6/6 | 0 |
| compare/delete_1row_1k/sqlite | series1 | 0.827 | 1.127 | 4/6 | n/a |
| compare/delete_1row_1k/sqlite | series2 | -2.034 | 1.648 | 1/6 | n/a |
| compare/update_1row_10k/lin | series1 | 14.045 | 3.359 | 6/6 | 0 |
| compare/update_1row_10k/lin | series2 | 12.574 | 3.008 | 6/6 | 0 |
| compare/update_1row_10k/sqlite | series1 | 0.243 | 3.998 | 3/6 | n/a |
| compare/update_1row_10k/sqlite | series2 | 0.876 | 2.784 | 3/6 | n/a |
| compare/delete_1row_10k/lin | series1 | 4.736 | 1.830 | 6/6 | 0 |
| compare/delete_1row_10k/lin | series2 | 4.232 | 1.505 | 5/6 | 0 |
| compare/delete_1row_10k/sqlite | series1 | -2.008 | 1.866 | 2/6 | n/a |
| compare/delete_1row_10k/sqlite | series2 | -5.736 | 0.971 | 2/6 | n/a |
| compare/update_1row_100k/lin | series1 | 16.752 | 2.007 | 6/6 | 0 |
| compare/update_1row_100k/lin | series2 | 13.195 | 3.467 | 6/6 | 0 |
| compare/update_1row_100k/sqlite | series1 | 2.144 | 4.651 | 3/6 | n/a |
| compare/update_1row_100k/sqlite | series2 | -7.798 | 16.761 | 1/6 | n/a |
| compare/delete_1row_100k/lin | series1 | 5.625 | 3.535 | 6/6 | 0 |
| compare/delete_1row_100k/lin | series2 | 0.486 | 3.987 | 3/6 | 0 |
| compare/delete_1row_100k/sqlite | series1 | 2.255 | 5.896 | 4/6 | n/a |
| compare/delete_1row_100k/sqlite | series2 | 17.925 | 6.769 | 4/6 | n/a |
| compare/tx_commit_1row_1k/lin | series1 | 13.339 | 1.698 | 6/6 | 0 |
| compare/tx_commit_1row_1k/lin | series2 | 11.515 | 3.099 | 6/6 | 0 |
| compare/tx_commit_1row_1k/sqlite | series1 | 0.507 | 0.976 | 3/6 | n/a |
| compare/tx_commit_1row_1k/sqlite | series2 | 0.891 | 3.171 | 6/6 | n/a |
| compare/tx_rollback_1row_1k/lin | series1 | 12.666 | 1.156 | 6/6 | 0 |
| compare/tx_rollback_1row_1k/lin | series2 | 14.187 | 1.565 | 6/6 | 0 |
| compare/tx_rollback_1row_1k/sqlite | series1 | 1.291 | 1.512 | 5/6 | n/a |
| compare/tx_rollback_1row_1k/sqlite | series2 | 0.122 | 2.700 | 3/6 | n/a |
| compare/tx_commit_1row_10k/lin | series1 | 5.874 | 1.816 | 6/6 | 0 |
| compare/tx_commit_1row_10k/lin | series2 | 8.987 | 2.189 | 6/6 | 0 |
| compare/tx_commit_1row_10k/sqlite | series1 | 2.197 | 5.939 | 4/6 | n/a |
| compare/tx_commit_1row_10k/sqlite | series2 | 2.575 | 7.299 | 4/6 | n/a |
| compare/tx_rollback_1row_10k/lin | series1 | 8.314 | 1.828 | 5/6 | 0 |
| compare/tx_rollback_1row_10k/lin | series2 | 8.676 | 1.606 | 6/6 | 0 |
| compare/tx_rollback_1row_10k/sqlite | series1 | 1.140 | 5.471 | 5/6 | n/a |
| compare/tx_rollback_1row_10k/sqlite | series2 | -4.363 | 3.862 | 1/6 | n/a |
| compare/update_1row_1k/lin | series1 | 18.461 | 3.215 | 6/6 | 0 |
| compare/update_1row_1k/lin | series2 | 15.732 | 3.868 | 6/6 | 0 |

Candidate qualified: False. All-eight-peer goal remains open.
