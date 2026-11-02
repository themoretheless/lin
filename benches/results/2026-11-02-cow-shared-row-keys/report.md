# Shared row keys: local discovery results

Positive gain means lower candidate latency. AA is the median absolute baseline/control difference. One local six-round series is not Linux/full-matrix qualification.

| Case | Gain % | AA % | Positive pairs |
|---|---:|---:|---:|
| compare/update_1row_1k/lin | +7.042 | 23.102 | 3/6 |
| compare/update_1row_10k/lin | -12.570 | 13.833 | 2/6 |
| compare/update_1row_100k/lin | -6.197 | 16.465 | 2/6 |
| compare/tx_rollback_1row_1k/lin | +14.964 | 10.046 | 4/6 |
| compare/tx_rollback_1row_10k/lin | +4.252 | 9.280 | 4/6 |
| compare/tx_rollback_1row_100k/lin | -3.118 | 6.376 | 3/6 |
| compare/insert_bulk_10k/lin | +5.342 | 1.748 | 6/6 |
| compare/delete_1row_10k/lin | +7.563 | 4.513 | 6/6 |
| compare/fts_lex_selective/lin | +15.265 | 5.907 | 6/6 |
| compare/agg_count_by/duckdb | +3.805 | 8.626 | 4/6 |
| compare/durable_update_1row_10k/lin | +3.766 | 8.034 | 3/6 |
| compare/durable_commit_1row_1k/lin | -4.812 | 7.889 | 2/6 |

Allocation proof: cloning twelve fields takes 3 allocations instead of 15 for both short and long keys. Full release correctness: 462 passed, 2 ignored, including the allocation test.

The candidate harness adapts only durable_json_row from k.clone() to k.to_string() for the vnext borrowed key API; this JSON conversion occurs in setup/verification. All timed workload code is unchanged. Compiler identity and build-input hashes are checked. DuckDB uses the exact baseline binary under every role label.

The host process snapshot at startup records concurrent unrelated Rust compilation, from other projects. This shared-host run is exploratory and cannot establish isolated-machine throughput or qualify adoption. The baseline/control measurements quantify observed variation but do not remove this limitation.

No adoption or all-peer superiority claim follows from this local experiment. SQLite FULL WAL, the six-peer corrected CI run, private MSSQL evidence and the missing real Kusto endpoint remain separate requirements.
