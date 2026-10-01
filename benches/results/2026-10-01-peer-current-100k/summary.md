# Current read benchmark summary

100k rows; three independent Python processes, rotated engine order, nine samples.
36/36 measured read comparisons favor Lin. 12/48 required comparisons are missing
because MSSQL and Kusto have no dedicated endpoint configured. Overall status: incomplete.

| Case | Lin µs | DuckDB µs | SQLite µs |
|---|---:|---:|---:|
| point_get | 0.250 | 49.434 | 1.038 |
| filter_eq | 0.797 | 479.454 | 699.030 |
| text_substr | 207.823 | 534.971 | 3550.750 |
| materialize | 6028.625 | 15795.958 | 21960.792 |
| join_inner | 14398.667 | 29825.042 | 52585.208 |
| join_filter | 7265.625 | 16029.416 | 27394.541 |

Exact values and duplicate multiplicities validated outside timing. Lin uses the
Rust prepared API; peers use Python DBAPI/DataFrame/driver APIs. Server queries
include network round-trip and result transfer. This is not isolated engine CPU
comparison. Background load is uncontrolled.

Current 10k ingestion still loses: Lin 14.636 ms vs SQLite 10.621 ms and DuckDB
Appender 10.480 ms in the separate native Rust run (bulk-fts-append). Default
Lin embedding/FTS and plain SQL fixture schemas perform different work.

[All peer results](report.md) and completion-audit.json retain missing-peer status.
