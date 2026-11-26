# Local ARM64 embedded peer refresh

Three fresh processes and five samples per case. Read queries use prepared Lin and the local SQLite and DuckDB APIs. Native INSERT uses in-memory databases and validates all six fields after timing. Medians pool process medians. This protocol does not isolate cold INSERT or measure durable commits.

| Rows | Mode | Process wins / comparisons | Lin INSERT ms | SQLite INSERT ms | DuckDB INSERT ms |
|---:|---|---:|---:|---:|---:|
| 1000 | read | 36/36 | — | — | — |
| 1000 | insert | 6/6 | 0.345 | 1.077 | 1.490 |
| 10000 | read | 36/36 | — | — | — |
| 10000 | insert | 6/6 | 3.131 | 12.995 | 8.379 |
| 100000 | read | 36/36 | — | — | — |
| 100000 | insert | 6/6 | 39.922 | 143.878 | 82.680 |
