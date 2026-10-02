# MySQL owned-table and native multi-row INSERT comparison

Final source forces ENGINE=InnoDB for all MySQL fixture tables. Each fixture has its own generated table prefix; static application docs/docs_bulk/users/orders/logs_bulk tables are not reused. Fixture Drop cleans only its owned names. Regular tables, indexes and transactions are used, not temporary or unlogged tables. CREATE/DROP TABLE privileges on the selected database suffice; no CREATE DATABASE privilege required by the benchmark.

Native mysql_batch: one server-prepared INSERT with all rows (6k or 60k parameters), preparation and schema outside timing; parameter encoding/allocation, transaction, execute and commit inside timing. Both rowwise and native insert cases read all six common fields from a separate connection outside timing and compare exact sorted tuples, including multiplicity. Matching common schema, indexes and identity constraints; Lin retains default hashing embedding, FTS and scalar index maintenance.

Dedicated Docker MySQL 8.4.11 with data on tmpfs, innodb_flush_log_at_trx_commit=1, sync_binlog=1, max_allowed_packet=64MiB. This includes client/server round-trip and transfer; it does not prove physical-disk durability or intrinsic engine CPU performance. COPY, LOAD DATA and alternative drivers are not measured here; no claim of best possible MySQL ingestion.

Three independent processes, eight fresh fixtures per case, one timed operation, no warmup. Schema, preparation, readback validation and input destruction excluded; output drop included. Fixed case order, uncontrolled host load. Medians within each process, then median of process medians. No compilation/tests overlapped timing. Initial run without explicit ENGINE is retained separately; final/ is authoritative.

| Rows | Lin ms | SQLite ms | DuckDB Appender ms | MySQL batch ms |
|---|---:|---:|---:|---:|
| 1k | 0.913333 | 0.787813 | 1.162604 | 5.987125 |
| 10k | 9.639209 | 10.168708 | 10.006250 | 68.459625 |

Safety: 11 selected */mysql cases complete in the dedicated validation database. Five sentinel tables and their exact rows remain unchanged after each of four final processes. Zero fixture tables remain. Native and rowwise insert readbacks pass. Validation database/grant removed; owned MySQL container stopped on completion. Verification, raw results and binary hashes in final/. Cargo bench compilation, scoped rustfmt and source diff checks pass. Library code unchanged; full library tests not rerun for this harness-only change.

The eight-peer target remains unproven: small inserts still lose to SQLite; DuckDB 10k comparison is not a stable Lin win; MSSQL/Kusto are unavailable; Mongo native ingestion is not covered. Missing peers are never counted as wins.
