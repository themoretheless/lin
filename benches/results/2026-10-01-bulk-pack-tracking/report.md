# Bulk insertion follow-up

One process, eight fresh fixtures per case; uncontrolled background load.

| 10k rows | Median |
|---|---:|
| Lin with default embedding and FTS | 12.536 ms |
| SQLite plain schema | 10.425 ms |
| DuckDB Appender plain schema | 10.438 ms |

Exact readback passed. Setup/preparation outside timing; appender flush inside.
Lin still loses at 10k. No universal peer win or causal allocation speedup is
claimed. This run precedes the subsequent FTS cancellation correctness fix.
