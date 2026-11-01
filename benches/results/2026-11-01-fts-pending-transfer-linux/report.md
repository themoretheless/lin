# FTS pending transfer: rejected

Original Linux run: https://github.com/themoretheless/lin/actions/runs/41336168475

756 fresh processes / 6048 observations. All functional tests and independent raw-data audit passed. No target window passed the frozen performance criteria; three control windows failed. No retry or production adoption.

All 4033 raw files are retained in raw.tar.gz and independently hashed in raw-manifest.json. Numerical analysis was recomputed from the downloaded observations and matched summary.json and qualification.json. Binary payloads are not archived; their hashes were checked against live binaries by CI.

| Case | Series | Gain % | Absolute A/A % | Positive pairs | Pass |
|---|---|---:|---:|---:|---|
| short\|compare/tx_rollback_1row_1k/lin | series1 | -4.012 | 16.607 | 2/6 | False |
| short\|compare/tx_rollback_1row_1k/lin | series2 | 6.863 | 11.351 | 4/6 | False |
| short\|compare/tx_rollback_1row_10k/lin | series1 | -1.647 | 6.535 | 3/6 | False |
| short\|compare/tx_rollback_1row_10k/lin | series2 | 2.251 | 3.450 | 5/6 | False |
| short\|compare/tx_rollback_1row_100k/lin | series1 | 1.140 | 3.862 | 4/6 | False |
| short\|compare/tx_rollback_1row_100k/lin | series2 | 1.742 | 3.086 | 3/6 | False |
| short\|compare/insert_bulk_1k/lin | series1 | 0.247 | 1.332 | 3/6 | True |
| short\|compare/insert_bulk_1k/lin | series2 | 0.602 | 0.669 | 4/6 | True |
| short\|compare/insert_bulk_10k/lin | series1 | -0.520 | 2.881 | 3/6 | True |
| short\|compare/insert_bulk_10k/lin | series2 | 0.664 | 2.838 | 3/6 | True |
| short\|compare/update_1row_1k/lin | series1 | -11.488 | 3.094 | 2/6 | False |
| short\|compare/update_1row_1k/lin | series2 | 1.642 | 4.451 | 4/6 | True |
| short\|compare/update_1row_10k/lin | series1 | -2.420 | 7.250 | 1/6 | True |
| short\|compare/update_1row_10k/lin | series2 | -1.517 | 4.066 | 3/6 | True |
| short\|compare/delete_1row_1k/lin | series1 | -1.666 | 4.643 | 3/6 | True |
| short\|compare/delete_1row_1k/lin | series2 | -1.312 | 1.972 | 2/6 | True |
| short\|compare/delete_1row_10k/lin | series1 | -2.173 | 2.164 | 2/6 | False |
| short\|compare/delete_1row_10k/lin | series2 | -1.472 | 4.828 | 2/6 | True |
| short\|compare/tx_commit_1row_10k/lin | series1 | 10.656 | 6.972 | 5/6 | True |
| short\|compare/tx_commit_1row_10k/lin | series2 | 3.425 | 2.641 | 5/6 | True |
| short\|compare/durable_update_1row_1k/lin | series1 | 4.098 | 6.144 | 4/6 | True |
| short\|compare/durable_update_1row_1k/lin | series2 | -2.130 | 3.074 | 3/6 | True |
| short\|compare/wal_scalar_full_update/lin | series1 | 1.651 | 2.743 | 4/6 | True |
| short\|compare/wal_scalar_full_update/lin | series2 | -1.763 | 2.328 | 3/6 | True |
| short\|compare/wal_scalar_normal_update/lin | series1 | -0.369 | 4.315 | 3/6 | True |
| short\|compare/wal_scalar_normal_update/lin | series2 | -0.564 | 2.287 | 2/6 | True |
| short\|compare/fts_lex_selective/lin | series1 | -1.397 | 2.286 | 2/6 | True |
| short\|compare/fts_lex_selective/lin | series2 | -0.893 | 3.881 | 2/6 | True |
| short\|compare/fts_lex_common/lin | series1 | 1.969 | 2.592 | 4/6 | True |
| short\|compare/fts_lex_common/lin | series2 | -1.327 | 1.463 | 2/6 | True |
| short\|compare/filter_eq/duckdb | series1 | 11.107 | 15.451 | 5/6 | True |
| short\|compare/filter_eq/duckdb | series2 | 3.374 | 7.520 | 4/6 | True |
| long\|compare/tx_rollback_1row_1k/lin | series1 | 9.688 | 2.351 | 6/6 | True |
| long\|compare/tx_rollback_1row_1k/lin | series2 | -0.791 | 1.960 | 2/6 | True |
| long\|compare/tx_rollback_1row_10k/lin | series1 | 4.904 | 3.950 | 5/6 | True |
| long\|compare/tx_rollback_1row_10k/lin | series2 | -7.309 | 8.321 | 1/6 | True |
| long\|compare/update_1row_1k/lin | series1 | 3.656 | 7.368 | 4/6 | True |
| long\|compare/update_1row_1k/lin | series2 | 23.736 | 17.002 | 6/6 | True |
| long\|compare/delete_1row_1k/lin | series1 | -5.083 | 8.632 | 0/6 | True |
| long\|compare/delete_1row_1k/lin | series2 | -3.170 | 2.454 | 1/6 | False |
| long\|compare/tx_commit_1row_1k/lin | series1 | 7.644 | 8.827 | 4/6 | True |
| long\|compare/tx_commit_1row_1k/lin | series2 | 1.619 | 2.376 | 3/6 | True |
