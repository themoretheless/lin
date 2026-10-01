# Validated PostgreSQL native insert and binary COPY

PostgreSQL rowwise transactional insert now uses bench_checked with complete committed readback of id, uri, wing, title, exact timestamp and body. Validation opens a separate connection and reads the owned fixture schema, outside the timer. New binary COPY cases preserve the same table/index constraints and explicit transaction commit; COPY row count checked and all fields read back. This avoids presenting rowwise network round trips as PostgreSQL bulk capability.

Three independent processes, eight fresh fixtures per case, one operation each. Setup/prepare and fixture cleanup excluded; actual write/commit included. Default Lin embeddings, FTS and CAS remain enabled. Fixed case order, uncontrolled host load; no significance claim. Milliseconds.

| Case | Process 1 | Process 2 | Process 3 | Median of process medians |
|---|---:|---:|---:|---:|
| compare/insert_bulk_1k/lin | 0.990771 | 0.925437 | 0.962959 | 0.962959 |
| compare/insert_bulk_1k/sqlite | 0.806271 | 0.840625 | 0.828105 | 0.828105 |
| compare/insert_bulk_1k/postgres | 225.301604 | 221.363479 | 234.260938 | 225.301604 |
| compare/insert_native_1k/postgres_copy | 4.328750 | 5.365105 | 4.715834 | 4.715834 |
| compare/insert_bulk_10k/lin | 10.257459 | 11.144000 | 10.101458 | 10.257459 |
| compare/insert_bulk_10k/sqlite | 10.679541 | 11.631626 | 10.586813 | 10.679541 |
| compare/insert_bulk_10k/postgres | 1974.377104 | 2510.812416 | 2028.728125 | 2028.728125 |
| compare/insert_native_10k/postgres_copy | 28.298229 | 30.476604 | 28.884771 | 28.884771 |
| compare/insert_native_1k/duckdb_appender | 1.300666 | 1.301896 | 1.436562 | 1.301896 |
| compare/insert_native_10k/duckdb_appender | 10.415125 | 10.501646 | 10.729833 | 10.501646 |

Lin beats tested PostgreSQL COPY at both sizes in all three processes. Lin 10k beats SQLite 3/3 and DuckDB Appender 2/3; 1k loses SQLite 3/3 and beats Appender 3/3. These scoped host/driver comparisons do not prove universal engine superiority or the all-eight-peer objective.

## Environment and cleanup

PostgreSQL 16.15; synchronous_commit=on, fsync=on, full_page_writes=on, wal_level=replica. Data directory is tmpfs under the existing Docker compose configuration, so this is not physical-disk/power-loss durability evidence. Native Lin/SQLite/DuckDB inputs are in memory; PostgreSQL still performs normal WAL transactions and driver/server round trips.

All runs completed and validation passed. Zero owned schemas remained after every process. Unique test database removed and pre-existing owned PostgreSQL container stopped afterward. verification.json records completion and binary hash. Offline native benchmark compile, scoped rustfmt and diff checks pass.

MySQL native helper still requires safe isolation; MSSQL/Kusto endpoints unavailable, other peer coverage not refreshed in this run. Goal remains active and unproven. Raw observations/logs and source hashes retained.
