# Same-binary extent WAL database component

Two six-round balanced Linux series. Positive gain means lower candidate latency; AA is median absolute baseline/control variation.

| Case | Gain S1 % | AA S1 % | Pairs S1 | Gain S2 % | AA S2 % | Pairs S2 |
|---|---:|---:|---:|---:|---:|---:|
| compare/durable_update_1row_1k/lin | +38.974 | 4.107 | 6/6 | +42.770 | 5.569 | 6/6 |
| compare/durable_update_1row_1k/sqlite_wal | +7.702 | 6.224 | 4/6 | -1.804 | 5.341 | 2/6 |
| compare/durable_delete_1row_1k/lin | +39.824 | 1.931 | 6/6 | +33.890 | 5.269 | 6/6 |
| compare/durable_delete_1row_1k/sqlite_wal | +0.322 | 2.838 | 3/6 | -1.480 | 7.430 | 2/6 |
| compare/durable_commit_1row_1k/lin | +41.480 | 4.942 | 6/6 | +36.689 | 4.921 | 6/6 |
| compare/durable_commit_1row_1k/sqlite_wal | -2.884 | 6.813 | 2/6 | -2.534 | 2.575 | 1/6 |
| compare/durable_rollback_1row_1k/lin | -0.018 | 2.946 | 3/6 | -2.827 | 4.362 | 2/6 |
| compare/durable_rollback_1row_1k/sqlite_wal | -1.174 | 1.228 | 2/6 | -0.166 | 1.536 | 2/6 |
| compare/durable_update_1row_10k/lin | +44.227 | 4.310 | 6/6 | +43.935 | 4.467 | 6/6 |
| compare/durable_update_1row_10k/sqlite_wal | -2.028 | 3.705 | 1/6 | +2.681 | 4.293 | 4/6 |
| compare/durable_delete_1row_10k/lin | +45.606 | 2.116 | 6/6 | +43.358 | 7.772 | 6/6 |
| compare/durable_delete_1row_10k/sqlite_wal | -1.234 | 7.810 | 3/6 | +2.001 | 1.959 | 4/6 |
| compare/durable_commit_1row_10k/lin | +45.726 | 6.101 | 6/6 | +44.184 | 4.662 | 6/6 |
| compare/durable_commit_1row_10k/sqlite_wal | +1.138 | 6.036 | 3/6 | -4.272 | 3.738 | 2/6 |
| compare/durable_rollback_1row_10k/lin | +0.162 | 2.576 | 3/6 | -1.138 | 1.397 | 1/6 |
| compare/durable_rollback_1row_10k/sqlite_wal | -1.374 | 4.302 | 2/6 | +2.008 | 1.309 | 4/6 |
| compare/update_1row_1k/lin | -4.683 | 1.649 | 2/6 | -0.396 | 0.507 | 3/6 |
| compare/update_1row_10k/lin | +1.394 | 2.027 | 4/6 | +0.305 | 2.000 | 3/6 |
| compare/update_1row_100k/lin | +1.387 | 3.912 | 4/6 | +1.102 | 2.902 | 3/6 |
| compare/insert_bulk_10k/lin | -0.249 | 0.788 | 2/6 | -0.312 | 0.757 | 2/6 |
| compare/fts_lex_selective/lin | +0.941 | 2.182 | 4/6 | -1.635 | 3.177 | 2/6 |

Matched SQLite FULL WAL comparisons: 77/96 wins. All 756 processes, 6048 observations, 84 smokes, immutable ELF and frozen Git blobs were independently checked.

This is a component experiment, not the full 67-case acceptance matrix. Setup/prewrite/verification costs are outside the inherited operation timing boundary and retained in driver elapsed times. All losses and control variation remain in the archive. No unchanged retry, adoption, power-loss qualification or full-objective completion follows. Later prepared-column/read-only/single-pass source changes are excluded from frozen commit 20a408f6c.
