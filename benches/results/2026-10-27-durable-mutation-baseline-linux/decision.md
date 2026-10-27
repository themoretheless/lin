# Decision: target the FULL WAL peer gap, retain both journal modes

Linux run40503479687 completed successfully on headcfdd02db425d432df98de40ba22577d6659021e0. Independent local raw audit confirms576 isolated processes,4608 measured wall observations,24 positive reopen smoke cases and384 same-round/label peer comparisons. Production Rust content is04749994; only the benchmark is extended. This is baseline qualification, not a source optimization or all-eight-peer victory. No candidate or API change is promoted.

All192 SQLite synchronous=FULL journal_mode=DELETE comparisons favour Lin. All192 SQLite synchronous=FULL journal_mode=WAL comparisons favour SQLite. Neither journal mode is dropped. Each cell below is the range of four six-round-series median ratios (two series ×two independent labels), not the ratio of pooled samples. Lin/SQLite >1 means Lin slower.

| Operation | Rows | Lin/SQLite FULL DELETE | Lin/SQLite FULL WAL | Lin WAL wins/24 |
|---|---:|---:|---:|---:|
| UPDATE | 1k | 0.460–0.496 | 1.233–1.345 | 0 |
| DELETE | 1k | 0.404–0.451 | 1.187–1.266 | 0 |
| Explicit commit | 1k | 0.423–0.455 | 1.170–1.322 | 0 |
| Rollback | 1k | 0.235–0.258 | 1.380–1.539 | 0 |
| UPDATE | 10k | 0.608–0.664 | 1.711–1.826 | 0 |
| DELETE | 10k | 0.554–0.627 | 1.627–1.795 | 0 |
| Explicit commit | 10k | 0.578–0.630 | 1.641–1.696 | 0 |
| Rollback | 10k | 0.286–0.302 | 1.401–1.646 | 0 |

Median Lin latency across the four reported process-series medians:1k UPDATE383.38us,DELETE385.30us,commit378.42us,rollback30.13us;10k UPDATE572.61us,DELETE577.19us,commit540.94us,rollback41.93us. These aggregate summaries do not replace the full per-series/raw evidence or A/A. Rollback is an in-memory undo operation in durable configuration, not an acknowledged durable write.

Next experiment must separate prepared mutation/undo, WAL encoding, submission/queueing, actual append, sync and completion costs on the actual production path and retain both SQLite modes. The larger10k gap is observed, but does not by itself identify filesystem, allocator, encoder or row-copy causality. Existing native CPU/COW microbench gains cannot be applied arithmetically as proof of closing this durable gap. Tracing can perturb timing; include an untraced same-source control and qualify final uninstrumented source with the unchanged native/read/durable controls. Do not weaken sync or omit generated row payloads/embedding/FTS/index work.

Clean-reopen verification is not crash/power-loss qualification. No PostgreSQL,MySQL,MSSQL,MongoDB,Kusto,DuckDB or pandas timing is produced by this particular matrix. The separate COW native/full-insert/read qualification remains its own run and source. Overall objective remains active and incomplete.
