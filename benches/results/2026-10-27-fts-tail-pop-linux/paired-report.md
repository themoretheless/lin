# Paired native mutation qualification

Two six-round series; target delete1k/10k/100k must win6/6 and exceed median absolute A/A. All unchanged controls and SQLite losses retained. This is not eight-peer or Full durable qualification.

| Case | Series | Gain % | A/A % | Positive pairs | SQLite wins |
|---|---|---:|---:|---:|---:|
| compare/update_1row_1k/lin | series1 | -0.722 | 1.704 | 2/6 | 0 |
| compare/update_1row_1k/lin | series2 | -1.139 | 1.863 | 2/6 | 0 |
| compare/update_1row_1k/sqlite | series1 | -0.117 | 2.339 | 3/6 | n/a |
| compare/update_1row_1k/sqlite | series2 | 1.726 | 3.979 | 4/6 | n/a |
| compare/delete_1row_1k/lin | series1 | -1.119 | 2.083 | 2/6 | 0 |
| compare/delete_1row_1k/lin | series2 | 0.826 | 1.710 | 4/6 | 0 |
| compare/delete_1row_1k/sqlite | series1 | 0.125 | 1.224 | 3/6 | n/a |
| compare/delete_1row_1k/sqlite | series2 | 2.976 | 3.480 | 5/6 | n/a |
| compare/update_1row_10k/lin | series1 | -0.395 | 2.148 | 3/6 | 0 |
| compare/update_1row_10k/lin | series2 | -2.005 | 1.743 | 1/6 | 0 |
| compare/update_1row_10k/sqlite | series1 | -11.340 | 2.717 | 2/6 | n/a |
| compare/update_1row_10k/sqlite | series2 | -10.018 | 5.469 | 1/6 | n/a |
| compare/delete_1row_10k/lin | series1 | -2.122 | 1.910 | 2/6 | 0 |
| compare/delete_1row_10k/lin | series2 | -3.555 | 1.852 | 1/6 | 0 |
| compare/delete_1row_10k/sqlite | series1 | -4.774 | 13.784 | 3/6 | n/a |
| compare/delete_1row_10k/sqlite | series2 | -1.361 | 10.509 | 3/6 | n/a |
| compare/update_1row_100k/lin | series1 | -3.971 | 2.457 | 3/6 | 0 |
| compare/update_1row_100k/lin | series2 | -1.129 | 6.537 | 3/6 | 0 |
| compare/update_1row_100k/sqlite | series1 | -16.956 | 9.854 | 1/6 | n/a |
| compare/update_1row_100k/sqlite | series2 | -2.816 | 4.433 | 1/6 | n/a |
| compare/delete_1row_100k/lin | series1 | -8.854 | 7.902 | 1/6 | 0 |
| compare/delete_1row_100k/lin | series2 | -2.960 | 3.055 | 2/6 | 0 |
| compare/delete_1row_100k/sqlite | series1 | -18.804 | 13.960 | 1/6 | n/a |
| compare/delete_1row_100k/sqlite | series2 | -2.492 | 5.679 | 3/6 | n/a |
| compare/tx_commit_1row_1k/lin | series1 | -3.288 | 3.567 | 1/6 | 0 |
| compare/tx_commit_1row_1k/lin | series2 | -4.594 | 3.547 | 2/6 | 0 |
| compare/tx_commit_1row_1k/sqlite | series1 | 1.146 | 1.014 | 3/6 | n/a |
| compare/tx_commit_1row_1k/sqlite | series2 | 1.113 | 1.595 | 5/6 | n/a |
| compare/tx_rollback_1row_1k/lin | series1 | -0.823 | 1.308 | 1/6 | 0 |
| compare/tx_rollback_1row_1k/lin | series2 | -3.333 | 1.953 | 2/6 | 0 |
| compare/tx_rollback_1row_1k/sqlite | series1 | -0.379 | 2.138 | 2/6 | n/a |
| compare/tx_rollback_1row_1k/sqlite | series2 | 1.943 | 2.046 | 4/6 | n/a |
| compare/tx_commit_1row_10k/lin | series1 | -3.333 | 0.979 | 1/6 | 0 |
| compare/tx_commit_1row_10k/lin | series2 | -3.830 | 3.710 | 1/6 | 0 |
| compare/tx_commit_1row_10k/sqlite | series1 | -1.612 | 12.488 | 2/6 | n/a |
| compare/tx_commit_1row_10k/sqlite | series2 | -0.520 | 6.424 | 3/6 | n/a |
| compare/tx_rollback_1row_10k/lin | series1 | -2.130 | 2.069 | 1/6 | 0 |
| compare/tx_rollback_1row_10k/lin | series2 | -2.602 | 1.358 | 1/6 | 0 |
| compare/tx_rollback_1row_10k/sqlite | series1 | 0.277 | 3.651 | 3/6 | n/a |
| compare/tx_rollback_1row_10k/sqlite | series2 | 3.093 | 2.041 | 6/6 | n/a |

Candidate qualified: False. All-eight-peer goal remains open.
