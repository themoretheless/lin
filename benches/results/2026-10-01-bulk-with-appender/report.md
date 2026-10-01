# Bulk ingestion with native DuckDB Appender

Eight samples, one process; preparation outside timing.
DuckDB Appender is flushed inside the timer and all six columns are read back and compared outside it.
Lin full docs insertion includes embedding and FTS; SQL adapters do not.
SQL row-loop and appender are separate cases. Results vary between runs; no independent process confidence is claimed.

| Case | Median ms |
|---|---:|
| compare/insert_bulk_1k/lin | 1.727 |
| compare/insert_bulk_1k/sqlite | 1.006 |
| compare/insert_bulk_1k/duckdb | 176.460 |
| compare/insert_bulk_10k/lin | 18.755 |
| compare/insert_bulk_10k/sqlite | 14.432 |
| compare/insert_bulk_10k/duckdb | 2757.613 |
| compare/insert_native_1k/duckdb_appender | 1.617 |
| compare/insert_native_10k/duckdb_appender | 13.708 |
