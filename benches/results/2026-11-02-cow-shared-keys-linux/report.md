# Shared row keys: dedicated Linux component results

Twelve cases, two balanced six-round series; 432 processes, 3456 observations, 48 smokes. Positive gain means lower latency. This is not full-matrix performance acceptance.

| Case | Gain S1 % | AA S1 % | Pairs S1 | Gain S2 % | AA S2 % | Pairs S2 |
|---|---:|---:|---:|---:|---:|---:|
| compare/update_1row_1k/lin | -5.801 | 4.182 | 1/6 | -2.691 | 3.496 | 1/6 |
| compare/update_1row_10k/lin | -0.754 | 1.129 | 1/6 | +1.672 | 5.686 | 4/6 |
| compare/update_1row_100k/lin | +0.917 | 7.376 | 4/6 | -0.666 | 4.119 | 3/6 |
| compare/tx_rollback_1row_1k/lin | +1.622 | 2.380 | 4/6 | +1.576 | 5.495 | 5/6 |
| compare/tx_rollback_1row_10k/lin | +3.082 | 2.171 | 5/6 | +0.969 | 2.253 | 5/6 |
| compare/tx_rollback_1row_100k/lin | +7.862 | 2.743 | 6/6 | +4.839 | 3.783 | 6/6 |
| compare/insert_bulk_10k/lin | +5.130 | 1.471 | 6/6 | +4.891 | 1.417 | 6/6 |
| compare/delete_1row_10k/lin | -1.270 | 2.156 | 2/6 | -1.536 | 2.131 | 2/6 |
| compare/fts_lex_selective/lin | +5.729 | 2.193 | 6/6 | +3.502 | 1.685 | 5/6 |
| compare/agg_count_by/duckdb | -1.724 | 2.632 | 1/6 | -1.534 | 5.026 | 2/6 |
| compare/durable_update_1row_10k/lin | -2.462 | 1.765 | 1/6 | -3.605 | 5.310 | 1/6 |
| compare/durable_commit_1row_1k/lin | -2.195 | 5.476 | 2/6 | -4.494 | 0.957 | 0/6 |

Bulk INSERT 10k and rollback 100k are positive in all six pairs in both series and exceed the median absolute AA variation. Regressions remain: UPDATE 1k S1 and FULL UPDATE 10k S1 exceed AA; FULL COMMIT 1k S2 is -4.494% against AA 0.957%, with zero positive pairs. Candidate not adopted; no unchanged retry or control waiver.

Both binaries use the exact same adapted harness and compiler. DuckDB uses the exact baseline executable for every role. Original source/test trees, full correctness counts, all raw data and exact ELF hashes were independently verified. These data do not prove wins over the named peer engines or resolve SQLite FULL WAL.

The initial local collector compared the Rust-only runtime snapshot against a Git manifest that also contained WGSL shaders. The collector now compares the same .rs scope, while independently verifying the complete source/test Git manifests (including WGSL) for both roles. The failed collector log and original downloads are preserved; no benchmark rerun or raw mutation occurred. All 2426 archive member hashes were verified.
