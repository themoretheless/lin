# Positive-zero Full WAL: rejected

Frozen source `9fb4c1ed321f73387d2f074e0547e9f5731d9e15`, candidate `62e14c7eb`, corrected baseline `21dcd7d81`. Run 41244869114 completed 432 processes and 3456 observations. Independent source, legacy-reader and observation audit passed. The frozen performance gate failed: zero failed target windows, two failed control windows.

| Case | Series | Gain % | Absolute A/A % | Positive pairs | Gate |
|---|---|---:|---:|---:|---|
| compare/durable_update_1row_1k/lin | series1 | 16.209 | 7.743 | 6/6 | PASS |
| compare/durable_update_1row_1k/lin | series2 | 20.562 | 7.010 | 6/6 | PASS |
| compare/durable_update_1row_10k/lin | series1 | 14.134 | 5.533 | 6/6 | PASS |
| compare/durable_update_1row_10k/lin | series2 | 13.727 | 10.555 | 6/6 | PASS |
| compare/durable_delete_1row_1k/lin | series1 | 20.337 | 3.202 | 6/6 | PASS |
| compare/durable_delete_1row_1k/lin | series2 | 17.928 | 6.392 | 6/6 | PASS |
| compare/durable_delete_1row_10k/lin | series1 | 15.555 | 4.216 | 6/6 | PASS |
| compare/durable_delete_1row_10k/lin | series2 | 11.556 | 5.224 | 6/6 | PASS |
| compare/durable_commit_1row_10k/lin | series1 | 15.936 | 5.582 | 6/6 | PASS |
| compare/durable_commit_1row_10k/lin | series2 | 16.322 | 4.291 | 6/6 | PASS |
| compare/durable_rollback_1row_10k/lin | series1 | 0.518 | 0.889 | 4/6 | PASS |
| compare/durable_rollback_1row_10k/lin | series2 | 1.856 | 1.931 | 5/6 | PASS |
| compare/wal_scalar_full_update/lin | series1 | 0.124 | 2.239 | 3/6 | PASS |
| compare/wal_scalar_full_update/lin | series2 | 0.338 | 4.270 | 3/6 | PASS |
| compare/wal_scalar_normal_update/lin | series1 | 1.001 | 1.315 | 5/6 | PASS |
| compare/wal_scalar_normal_update/lin | series2 | -0.298 | 2.826 | 3/6 | PASS |
| compare/update_1row_1k/lin | series1 | -3.214 | 1.096 | 2/6 | FAIL |
| compare/update_1row_1k/lin | series2 | -2.323 | 1.967 | 1/6 | FAIL |
| compare/insert_bulk_10k/lin | series1 | 0.068 | 2.097 | 3/6 | PASS |
| compare/insert_bulk_10k/lin | series2 | -1.428 | 0.968 | 2/6 | PASS |
| compare/filter_eq/lin | series1 | 1.502 | 2.732 | 6/6 | PASS |
| compare/filter_eq/lin | series2 | 1.348 | 1.873 | 4/6 | PASS |
| compare/durable_update_1row_10k/sqlite_wal | series1 | 1.472 | 3.060 | 3/6 | PASS |
| compare/durable_update_1row_10k/sqlite_wal | series2 | -2.017 | 3.805 | 1/6 | PASS |

All ten Full UPDATE/DELETE/commit target windows passed, with median gains of 11.556–20.562%. Native in-memory UPDATE failed in both series: -3.214% and -2.323%. Its frozen setup calls `setup_lin_write(n)` -> `Db::empty()` -> `Db::bare`, with no persistence. The changed encoder is therefore not executed in this control. The cause of the observed regression is not established; a code-layout or compiler explanation remains a hypothesis, not evidence for waiving the failure.

Correctness evidence: 457 release tests passed, two ignored; 24 fresh Linux smoke processes; eight actual production-reader golden cases / 32 exact golden files; two separately compiled runtime mutants rejected. No memory measurement or new peer comparison was performed.

All 2385 raw files are retained in raw.tar.gz with per-member SHA256 and verified extraction hashes. No observation was removed. No unchanged rerun or threshold relaxation is justified. Runtime candidate remains isolated on the experimental branch and has not been accepted for production. The full-source COW regressions, SQLite FULL WAL gaps and eight-peer objective remain open.
