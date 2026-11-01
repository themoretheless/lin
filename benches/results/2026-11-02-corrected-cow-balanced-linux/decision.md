# Corrected balanced COW source comparison: rejected

Run 41353987944 completed with failure only at the unchanged source acceptance gate. All 2412 fresh processes / 19296 observations, 134 functional smokes, exact source/prerequisite audits, raw command/numerical verification and per-case role balance passed. Local numerical reconstruction matches CI. All 15243 archived files rehashed. Linux executable payloads are not archived.

| Target | Series | Gain % | A/A % | Positive |
| --- | --- | ---: | ---: | ---: |
| compare/update_1row_1k/lin | series1 | 55.189 | 3.164 | 6/6 |
| compare/update_1row_1k/lin | series2 | 56.261 | 1.778 | 6/6 |
| compare/update_1row_10k/lin | series1 | 53.396 | 4.479 | 6/6 |
| compare/update_1row_10k/lin | series2 | 50.899 | 3.130 | 6/6 |
| compare/update_1row_100k/lin | series1 | 52.950 | 2.608 | 6/6 |
| compare/update_1row_100k/lin | series2 | 56.828 | 4.203 | 6/6 |
| compare/wal_scalar_full_update/lin | series1 | 63.342 | 2.115 | 6/6 |
| compare/wal_scalar_full_update/lin | series2 | 63.167 | 2.209 | 6/6 |
| compare/wal_scalar_normal_update/lin | series1 | 97.529 | 4.358 | 6/6 |
| compare/wal_scalar_normal_update/lin | series2 | 97.673 | 1.566 | 6/6 |

All ten target windows pass. Four control windows fail:

- compare/durable_commit_1row_1k/lin series2: -6.792% gain vs 6.343% A/A; 2/6 positive.
- compare/durable_delete_1row_10k/sqlite_delete series1: -10.839% gain vs 4.965% A/A; 0/6 positive.
- compare/agg_count_by/duckdb series1: -14.419% gain vs 8.901% A/A; 3/6 positive.
- compare/agg_count_by/duckdb series2: -14.991% gain vs 3.870% A/A; 0/6 positive.

These failures are retained. The DuckDB GROUP BY regression appears in both series; its cause is unproven. No external-control waiver or noise attribution.

Candidate numeric peer wins: 245/456. SQLite FULL WAL: 0/96 wins across UPDATE, DELETE, COMMIT and ROLLBACK at 1k/10k. Paired median Lin/SQLite ratios by series follow.

| Case | Series | Lin/SQLite | Wins |
| --- | --- | ---: | ---: |
| compare/durable_update_1row_1k/lin | series1 | 1.409202 | 0/6 |
| compare/durable_update_1row_1k/lin | series2 | 1.372051 | 0/6 |
| compare/durable_delete_1row_1k/lin | series1 | 1.398841 | 0/6 |
| compare/durable_delete_1row_1k/lin | series2 | 1.352070 | 0/6 |
| compare/durable_commit_1row_1k/lin | series1 | 1.269049 | 0/6 |
| compare/durable_commit_1row_1k/lin | series2 | 1.453251 | 0/6 |
| compare/durable_rollback_1row_1k/lin | series1 | 1.550976 | 0/6 |
| compare/durable_rollback_1row_1k/lin | series2 | 1.607834 | 0/6 |
| compare/durable_update_1row_10k/lin | series1 | 1.615922 | 0/6 |
| compare/durable_update_1row_10k/lin | series2 | 1.542017 | 0/6 |
| compare/durable_delete_1row_10k/lin | series1 | 1.700142 | 0/6 |
| compare/durable_delete_1row_10k/lin | series2 | 1.747049 | 0/6 |
| compare/durable_commit_1row_10k/lin | series1 | 1.650555 | 0/6 |
| compare/durable_commit_1row_10k/lin | series2 | 1.684997 | 0/6 |
| compare/durable_rollback_1row_10k/lin | series1 | 1.289948 | 0/6 |
| compare/durable_rollback_1row_10k/lin | series2 | 1.306944 | 0/6 |

No source adoption, main merge or unchanged retry. The original unbalanced run 41306613457 remains failed and separately archived. The API transition is authorized for the next version, but performance qualification and superiority over all eight peers remain open.
