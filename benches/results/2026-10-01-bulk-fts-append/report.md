# Peer insertion after retained FTS append optimization

One process, 12 fresh fixtures per case, uncontrolled background load.

| Rows | Lin | SQLite | DuckDB Appender |
|---|---:|---:|---:|
| 1k | 1.096 ms | 0.781 ms | 1.232 ms |
| 10k | 14.636 ms | 10.621 ms | 10.480 ms |

Exact appender readback passed; setup/preparation outside timing, flush inside.
Lin includes default embedding and FTS; SQL engines have the plain fixture schema.
Lin still loses to both optimized SQL ingestion paths at 10k. The retained FTS
improvement is established against the original Lin in the separate alternating
pairs, not by comparing this run to older peer reports. MSSQL/Kusto unverified.
