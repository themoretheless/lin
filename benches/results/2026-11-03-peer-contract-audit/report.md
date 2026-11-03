# Peer scope and durability contract audit

The objective remains beating DuckDB, SQLite, PostgreSQL, MySQL, MSSQL, MongoDB, Kusto and pandas. An internal source gate, an engine-subset win, or a functional CI pass does not prove this complete objective. Runtime and contract boundaries below are binding.

| Evidence | Runtime / scope | What it establishes | What remains open |
|---|---|---|---|
| Public run 41567995224 | f37 runtime; SQLite/DuckDB/PostgreSQL/MySQL/MongoDB/pandas; prepared reads and native insertion | 108/108 aggregate read wins, 324/324 read-process wins; 13/15 native aggregate wins, 39/45 native process wins | DuckDB native INSERT 10k/100k loses every process; no durability-equivalent comparison |
| Encrypted MSSQL run 41568756571 | f37 runtime, real SQL Server | Independently verified complete 18-process / 1512-sample / 21-comparison functional and source matrix | Numeric outcomes stay in encrypted evidence; public green status does not imply a win |
| Linux phase profile 41590218518 | f37 runtime; ordinary and diagnostic workers | Six processes / 18 wall samples, exact values and source/ELF closures; embedding generation dominates measured 100k work | Diagnostic attribution is not a candidate speedup gate |
| Caller-participating embedding gate 41591451842 | new 01e12cfa9 runtime, baseline differs only in src/embed.rs | Frozen prospective INSERT/A-A/read-control protocol; execution is pending | No qualification transfers automatically from f37; all downstream peers still require current-runtime proof |
| Root compare durable INSERT | Lin Db::open WAL path versus SQLite file transaction | SQLite is configured synchronous=FULL and journal_mode=DELETE in benches/compare.rs | This code does not establish SQLite FULL WAL performance or equivalent checkpoint/commit contracts |
| Kusto | No configured endpoint in current local environment or repo LIN_BENCH_KUSTO secret-name inventory; no connector | No current Kusto measurement exists | Real cluster URL, database and authorized access are needed; no emulator substitutes |

Default Lin embeddings/FTS/indexes remain enabled in the native workload. Native inserts are prepared, and 100k uses ten bounded 10k statements in one memory transaction. Current public peer timing does not establish durable commit-level equality, RSS/cold-query behavior or every SQL workload. Pandas is a read peer in this protocol, not a native insertion backend.

Independent preserved evidence is in sibling directories current-candidate-public-peers-linux, current-candidate-mssql-private and insert-linux-profile-evidence (each prefixed 2026-11-03). The complete default workspace passed locally for caller runtime (415 passed, zero failed, three ignored), but the original frozen Linux job remains authoritative for its own target/control result. No benchmark is restarted to improve an unchanged numerical outcome.

Durable source inspection: setup_lin_durable_insert uses Db::open and prepared insert with index docs [wing, ts]; setup_sqlite_durable explicitly uses FULL + DELETE, creates the same public six data fields and a wing/ts index, and commits through fill_sqlite. Journal mode and Lin's additional embedding behavior must be reported instead of treating the names as interchangeable contracts. A future FULL WAL benchmark needs explicit pragma readback, sync settings, commit boundaries and exact value/reopen validation.
