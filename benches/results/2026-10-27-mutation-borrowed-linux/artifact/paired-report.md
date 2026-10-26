# Paired native mutation qualification

Two six-round series; target update1k/10k/100k must win6/6 and exceed median absolute A/A. All unchanged controls and SQLite losses retained. This is not eight-peer or Full durable qualification.

| Case | Series | Gain % | A/A % | Positive pairs | SQLite wins |
|---|---|---:|---:|---:|---:|
| compare/update_1row_1k/lin | series1 | 6.005 | 0.984 | 5/6 | 0 |
| compare/update_1row_1k/lin | series2 | 7.037 | 2.146 | 6/6 | 0 |
| compare/update_1row_1k/sqlite | series1 | 0.613 | 2.571 | 4/6 | n/a |
| compare/update_1row_1k/sqlite | series2 | 4.159 | 1.287 | 6/6 | n/a |
| compare/delete_1row_1k/lin | series1 | -0.109 | 1.537 | 3/6 | 0 |
| compare/delete_1row_1k/lin | series2 | 0.637 | 1.767 | 5/6 | 0 |
| compare/delete_1row_1k/sqlite | series1 | -1.025 | 2.742 | 2/6 | n/a |
| compare/delete_1row_1k/sqlite | series2 | 3.290 | 1.171 | 6/6 | n/a |
| compare/update_1row_10k/lin | series1 | 4.298 | 1.006 | 4/6 | 0 |
| compare/update_1row_10k/lin | series2 | 1.972 | 1.523 | 5/6 | 0 |
| compare/update_1row_10k/sqlite | series1 | -4.803 | 4.656 | 2/6 | n/a |
| compare/update_1row_10k/sqlite | series2 | 5.950 | 8.890 | 4/6 | n/a |
| compare/delete_1row_10k/lin | series1 | -1.497 | 2.795 | 3/6 | 0 |
| compare/delete_1row_10k/lin | series2 | 1.770 | 3.482 | 4/6 | 0 |
| compare/delete_1row_10k/sqlite | series1 | 0.196 | 4.344 | 3/6 | n/a |
| compare/delete_1row_10k/sqlite | series2 | -0.399 | 7.220 | 2/6 | n/a |
| compare/update_1row_100k/lin | series1 | 3.797 | 3.065 | 5/6 | 0 |
| compare/update_1row_100k/lin | series2 | 4.213 | 3.176 | 4/6 | 0 |
| compare/update_1row_100k/sqlite | series1 | 0.574 | 24.237 | 4/6 | n/a |
| compare/update_1row_100k/sqlite | series2 | 11.524 | 15.000 | 5/6 | n/a |
| compare/delete_1row_100k/lin | series1 | 2.798 | 1.647 | 5/6 | 0 |
| compare/delete_1row_100k/lin | series2 | -2.533 | 2.230 | 1/6 | 0 |
| compare/delete_1row_100k/sqlite | series1 | 12.675 | 10.016 | 4/6 | n/a |
| compare/delete_1row_100k/sqlite | series2 | 10.924 | 6.766 | 5/6 | n/a |
| compare/tx_commit_1row_1k/lin | series1 | 7.254 | 3.367 | 5/6 | 0 |
| compare/tx_commit_1row_1k/lin | series2 | 5.424 | 4.999 | 6/6 | 0 |
| compare/tx_commit_1row_1k/sqlite | series1 | 2.893 | 4.636 | 4/6 | n/a |
| compare/tx_commit_1row_1k/sqlite | series2 | 1.590 | 3.189 | 4/6 | n/a |
| compare/tx_rollback_1row_1k/lin | series1 | 5.875 | 2.177 | 6/6 | 0 |
| compare/tx_rollback_1row_1k/lin | series2 | 2.399 | 1.527 | 5/6 | 0 |
| compare/tx_rollback_1row_1k/sqlite | series1 | 1.918 | 3.069 | 4/6 | n/a |
| compare/tx_rollback_1row_1k/sqlite | series2 | 0.532 | 0.938 | 3/6 | n/a |
| compare/tx_commit_1row_10k/lin | series1 | 1.598 | 1.107 | 6/6 | 0 |
| compare/tx_commit_1row_10k/lin | series2 | 2.959 | 4.113 | 6/6 | 0 |
| compare/tx_commit_1row_10k/sqlite | series1 | -0.115 | 2.998 | 3/6 | n/a |
| compare/tx_commit_1row_10k/sqlite | series2 | 1.732 | 2.562 | 5/6 | n/a |
| compare/tx_rollback_1row_10k/lin | series1 | 1.992 | 2.991 | 4/6 | 0 |
| compare/tx_rollback_1row_10k/lin | series2 | 0.608 | 3.910 | 4/6 | 0 |
| compare/tx_rollback_1row_10k/sqlite | series1 | -6.807 | 2.573 | 2/6 | n/a |
| compare/tx_rollback_1row_10k/sqlite | series2 | 2.504 | 3.529 | 5/6 | n/a |

Candidate qualified: False. All-eight-peer goal remains open.
