# Bulk insertion after WAL and affected-count fix

One process, eight fresh fixtures per case. Lin checks affected count, total rows,
static fields, timestamp type and embedding presence outside timing. DuckDB
Appender exact readback also passed. Setup/preparation outside timing; flush inside.

| Rows | Lin ms | SQLite ms | DuckDB Appender ms |
|---|---:|---:|---:|
| 1k | 1.166 | 0.801 | 1.182 |
| 10k | 12.762 | 10.585 | 10.205 |

Lin performs default embedding and FTS; SQL peers have the plain fixture schema.
Lin still loses the 10k ingestion comparisons. This run establishes stronger
correctness checks, not a causal speedup. Background load is uncontrolled.

The separate WAL regression confirms all 129 inserted rows/fields/embeddings
reach a fresh replica even though Handle.rows is elided. The full peer goal
remains incomplete; MSSQL and Kusto lack configured dedicated endpoints.
