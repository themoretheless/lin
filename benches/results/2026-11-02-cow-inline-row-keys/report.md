# Inline row keys: local discovery results

Positive gain means lower candidate latency. AA is the median absolute baseline/control difference. One local six-round series is not Linux/full-matrix qualification.

| Case | Gain % | AA % | Positive pairs |
|---|---:|---:|---:|
| compare/update_1row_1k/lin | -23.466 | 14.691 | 1/6 |
| compare/update_1row_10k/lin | -9.202 | 5.627 | 2/6 |
| compare/update_1row_100k/lin | +6.891 | 5.640 | 4/6 |
| compare/tx_rollback_1row_1k/lin | -16.339 | 12.185 | 2/6 |
| compare/tx_rollback_1row_10k/lin | -5.394 | 6.494 | 2/6 |
| compare/tx_rollback_1row_100k/lin | +4.337 | 1.335 | 5/6 |
| compare/insert_bulk_10k/lin | -3.514 | 3.277 | 2/6 |
| compare/delete_1row_10k/lin | +8.326 | 6.423 | 5/6 |
| compare/fts_lex_selective/lin | +13.566 | 5.681 | 5/6 |
| compare/agg_count_by/duckdb | +0.093 | 3.954 | 3/6 |
| compare/durable_update_1row_10k/lin | -4.234 | 7.589 | 2/6 |
| compare/durable_commit_1row_1k/lin | -12.494 | 2.449 | 1/6 |

Allocation proof: cloning twelve fields takes 3 allocations instead of 15 for both short and long keys. Full release correctness: 461 passed, 2 ignored, plus the new allocation test passed separately.

The candidate harness adapts only durable_json_row from k.clone() to k.to_string() for the vnext borrowed key API; this JSON conversion occurs in setup/verification. All timed workload code is unchanged. Compiler identity and build-input hashes are checked. DuckDB uses the exact baseline binary under every role label.

The host process snapshot at startup records concurrent unrelated Rust compilation, including a process using over seven CPU cores. This shared-host run is exploratory and cannot establish isolated-machine throughput or qualify adoption. The baseline/control measurements quantify observed variation but do not remove this limitation.

No adoption or all-peer superiority claim follows from this local experiment. SQLite FULL WAL, the six-peer corrected CI run, private MSSQL evidence and the missing real Kusto endpoint remain separate requirements.
