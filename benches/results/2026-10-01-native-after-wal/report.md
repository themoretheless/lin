# Native insertion after retained WAL changes

Three independent process runs; 24 fresh fixtures per case, one operation each. Exact input timestamps and row validation unchanged. Native full Lin insert includes default hashing embeddings, FTS and CAS; competitor paths use their checked native bulk interfaces. Setup and teardown outside measurement. Pinned current production binary. Fixed case order and uncontrolled host load. Milliseconds.

| Case | Process 1 | Process 2 | Process 3 | Median of process medians |
|---|---:|---:|---:|---:|---:|
| compare/insert_bulk_1k/lin | 1.031875 | 1.012500 | 1.001625 | 1.012500 |
| compare/insert_bulk_1k/sqlite | 0.789041 | 0.808480 | 0.787896 | 0.789041 |
| compare/insert_bulk_10k/lin | 10.664583 | 10.599083 | 10.646521 | 10.646521 |
| compare/insert_bulk_10k/sqlite | 10.353438 | 10.510562 | 10.451708 | 10.451708 |
| compare/insert_native_1k/duckdb_appender | 1.144729 | 1.178271 | 1.221958 | 1.178271 |
| compare/insert_native_10k/duckdb_appender | 10.205291 | 10.315188 | 10.448563 | 10.315188 |
| compare/insert_phase_10k/lin_full | 10.630042 | 11.710021 | 11.048958 | 11.048958 |
| compare/insert_phase_10k/lin_no_embed | 7.336562 | 8.173855 | 8.652104 | 8.173855 |
| compare/insert_phase_10k/lin_no_embed_no_scalar_index | 6.698562 | 6.624646 | 6.601771 | 6.624646 |
| compare/insert_phase_10k/lin_no_embed_no_scalar_no_fts | 5.216458 | 5.132062 | 5.216625 | 5.216458 |

Full native 10k Lin remains slower than SQLite and DuckDB Appender. Full native 1k beats Appender but loses SQLite. Phase cases disable named Lin components solely for diagnosis; they are not peer-equivalence or completion evidence. Differences across phases are not isolated causal timing proof.

PostgreSQL/MySQL unavailable to this native harness; explicitly excluded to avoid static-table helpers. MongoDB/pandas are covered by a separate peer harness, not these native write measurements. MSSQL/Kusto environment configuration is absent in this turn (presence booleans only; no secrets recorded). No overall superiority over all eight peers proven. Next experiment targets feature-hash modulo cost at default dimension, preserving bit-identical slot selection.
