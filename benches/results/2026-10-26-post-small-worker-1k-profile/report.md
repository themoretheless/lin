# Durable1k profile after small worker acceptance

Frozena5e28e36; profiling-only executable. Timers/logging added to copies of
exec/persist/store, then the original source restored byte-for-byte. Source,
compiler and binary hashes retained. Three native and three durable release
processes,24 one-operation samples each plus preflights. Exact normal benchmark
fixture validation remains enabled. Instrumentation changes scheduling/codegen;
these numbers diagnose cost, not a baseline/candidate speedup.

## Median of process medians, microseconds

| Span | Native | Full durable |
|---|---:|---:|
| append_sparse_rows_serial_ns | — | 199.833 |
| attach_ns | 24.458 | 25.125 |
| build_ns | 146.583 | 149.542 |
| checks_ns | 102.250 | 109.167 |
| checksum_ns | — | 32.584 |
| drop_pack_ns | — | 36.583 |
| embed_ns | 332.958 | 359.166 |
| encode_insert_cols_v2_ns | — | 342.083 |
| fts_ns | 139.042 | 143.416 |
| generate_ns | 200.250 | 215.208 |
| hnsw_ns | 0.041 | 0.041 |
| index_ns | 97.417 | 102.875 |
| maintenance_total_ns | — | 305.375 |
| maps_ns | 34.125 | 36.541 |
| rows_to_insert_cols_ns | — | 134.834 |
| submit_ns | — | 25.833 |
| sync_ns | — | 446.625 |
| text_ns | 58.708 | 59.208 |
| use_sparse_vectors_ns | — | 95.167 |
| wait_ns | — | 213.583 |
| write_ns | — | 39.250 |

Worker encoding and main maintenance overlap. wait_ns follows maintenance;
submit_ns includes lazy worker acquisition/cold startup. Encoding contains
classification and sparse emission; embedding contains text/generation/attach.
Nested spans and independently aggregated medians must not be added/subtracted
to claim a critical path or total latency. Logging on both threads perturbs the
schedule. Pack-drop timer explicitly drops the committed record at its normal
end-of-function lifetime, after durability, and measures that release alone.

Observed durable vector classification≈95us and sparse emission≈200us still
consume separate passes. A bit-exact SIMD classifier is a testable hypothesis;
CPU/full-engine controls are required. Sync≈447us stays enabled; no weaker sync,
lossy embeddings or omitted indexes/features are proposed. Durable SQLite1k
and real Kusto qualification remain open.

Phase-log medians include all logged invocations, including untimed validation preflights; per-span event counts remain in phase-medians.json. They are not medians of only the timed samples.
