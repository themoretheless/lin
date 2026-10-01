# Absolute timestamps and exact native-ingestion input parity

Added timestamp(signed_i64) syntax, representing Unix milliseconds with the existing time type and Cell::Time. Bare timestamp retains its former name parsing semantics. Type checking, canonical formatting, AST hashing, execution and scalar seek bounds handle the literal. Existing row/WAL/snapshot encodings are unchanged. README documents the syntax.

The benchmark source now uses Doc.ts directly for every Lin record. The warm fixture mutation/rebuild workaround was removed: assertions verify the values produced by actual inserts, including two [wing, ts] keys. Native Lin insertion validates exact timestamps rather than just checking Cell::Time. SQLite now has complete six-field readback validation like DuckDB Appender. Validation runs outside timing; no application result cache was introduced.

## Verification

The complete default workspace suite passed. New tests cover i64 boundaries, negative epochs, invalid float/duration/overflow forms, unchanged bare-name parsing, scan/index equality and ranges, prepared statements, bulk WAL replication, checkpoint and reopen. cargo check passed. Existing rollback, FTS, cold-store and large-insert count/WAL tests remain green.

The first broad benchmark selector accidentally included durable insertion and terminated at the existing 16MiB WAL-record ceiling. That incomplete run is excluded in excluded-run.json. The final precise selector contains only the six in-memory Lin/SQLite/DuckDB-Appender cases (selection.log); all completed with passing readback validators. This does not close the durable insertion requirement: its frame-size constraint still needs work.

## Exact-value native insertion snapshot

One process, 12 fresh fixtures per case, one operation each. Setup/input destruction and readback are outside timing; Appender flush is inside timing. Output types have trivial destructors. Shared host load is uncontrolled; this is not independent-process or statistical proof of a peer win. Earlier timing snapshots lacking exact timestamp checks are not equivalent exact-value evidence.

| Rows | Lin ms | SQLite ms | DuckDB Appender ms |
|---|---:|---:|---:|
| 1k | 1.028 | 0.820 | 1.263 |
| 10k | 10.990 | 10.488 | 10.443 |

No complete eight-peer victory is claimed. Native insertion/deletion optimization, the durable frame-size case, and live MSSQL/Kusto comparison remain outstanding. This change establishes exact data parity and a usable absolute-time input API; it is not a claimed performance speedup.

Raw observations, selection, build/test logs, excluded-run reason, before-source copies and final source hashes are retained.
