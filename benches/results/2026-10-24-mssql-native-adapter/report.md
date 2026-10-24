# SQL Server native bulk insertion adapter: implemented, live gate pending

Adds the previously missing MSSQL insert_native path to scripts/bench-peers.py. No Rust engine change and no new performance measurements or peer win claimed. Dedicated SQL Server configuration remains absent in the current environment.

## Implemented contract

Use installed PyTDS copy_to with explicitly typed NVARCHAR id/uri/wing/title/body and BIGINT timestamp columns. Fresh task-owned dbo table per sample, primary id, unique uri and wing/ts index. Setup/column metadata and exact six-field/multiplicity readback/drop are outside timing. Time explicit BEGIN TRANSACTION, typed bulk-copy transfer/serialization and COMMIT TRANSACTION. CHECK_CONSTRAINTS, FIRE_TRIGGERS and KEEP_NULLS enabled. Record ProductVersion, current database recovery model and delayed-durability policy; do not weaken those settings. SET XACT_ABORT ON is session-local error/rollback handling, not a relaxed durability setting.

Driver connection is dedicated/autocommit for setup and explicit SQL transaction control. Bracket-quote identifiers with the driver's helper; reject identifiers over 128 characters. A successful CREATE marks ownership; CREATE failure never drops the possibly preexisting table. After success/failure, attempt rollback if any transaction remains, then DROP the owned table even if rollback itself fails. Any copy/readback/cleanup error prevents a successful measurement report. Close the dedicated connection on constructor failure or completion. A broken/unavailable server can still reject cleanup; this is not a guarantee that tables disappear after all transport failures.

This is native bulk API comparison with Db::empty, not equal disk durability. Server storage/network/driver cost, NVARCHAR and default collation differ from Lin. No server-wide/database settings changed. The driver's copy_to API is marked experimental by its installed docstring; its real version/signature/source hash are recorded in driver-api.json. Live typed-wire/server/transaction validation is still required.

## Verification

The new test failed before implementation because MSSQL native insertion was unsupported. All 15 adapter tests now pass. Real installed driver metadata classes are used while transport/connection are mocked: success with two fresh samples, bulk failure, wrong-value duplicate readback with the same row count, CREATE failure preservation, and rollback failure with DROP still attempted. Fixtures include Unicode, an emoji, quote/comma/tab/newline text and timestamp; asserted NVARCHAR/BIGINT metadata, explicit BEGIN/COMMIT order boundaries, constraints/triggers enabled and owned cleanup. This is client control-flow/type metadata evidence, not server SQL syntax execution or bulk-wire qualification.

A real CLI invocation with MSSQL variables deliberately removed exits 2, status incomplete, no observations/medians and an explicit missing-configuration error. It is retained under unconfigured-run and is not counted as a peer victory. Existing actual SQLite/DuckDB and independent-process/gate tests still pass. The unchanged Rust workspace/performance matrix was not rerun for this Python-only adapter addition.

## Live gate still needed

Configure LIN_BENCH_MSSQL_HOST/PORT/USER/PASSWORD/DATABASE for a dedicated permitted instance; first validate the read and insert_native contracts at small size, then 1k/10k/100k with independent repetitions, version/settings/hash evidence and fixture/sentinel cleanup checks. An available endpoint alone does not count as a win. Kusto native insertion remains a separate missing contract; its emulator cannot substitute for permitted service benchmarks. Durable 1k SQLite and the full named-peer goal remain open.
