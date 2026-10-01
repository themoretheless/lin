# Final serial bulk verification

One process, 12 fresh-fixture samples per case. Setup is outside timing. Lin affected count and every row are checked outside timing; DuckDB Appender includes flush inside timing and checks stored fields outside timing.

| Rows | Lin | SQLite | DuckDB Appender |
|---|---:|---:|---:|
| 1k | 1.759 ms | 1.110 ms | 1.501 ms |
| 10k | 15.738 ms | 13.927 ms | 12.193 ms |

Lin still loses these ingestion cases. The slow DuckDB SQL row-loop case does not establish a win against its native Appender. Lin computes embeddings and maintains its native FTS/indexes; peer schemas/work differ as documented in the benchmark contract. Background load is uncontrolled; these 12 samples are not 12 independent processes.

Retained local optimizations have separate alternating-process evidence: sparse normalization (`../2026-10-01-norm-alternating/report.md`) and FTS append (`../2026-10-01-fts-append-ab/report.md`). Pool and scoped-thread experiments were reverted.

Large-insert WAL/readback safety and correct affected counts remain fixed. No universal peer win or goal completion is claimed.
