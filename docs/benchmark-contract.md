# Benchmark contract

The performance target is a measured win for Lin on explicitly defined workloads
against SQLite, DuckDB, PostgreSQL, MySQL, MongoDB, SQL Server, Kusto, and pandas.
A missing engine, incorrect result, or incompatible workload is not a win.
This is not a claim of superiority for every workload or data size.

## Native Rust comparison

Benchmark/example dev dependencies (`airbug 0.6`) require Rust 1.96 or newer;
benchmark/compute CI pins Rust 1.96.0. The library package MSRV remains a separate
contract in Cargo.toml. Do not infer benchmark MSRV from the library field.

`cargo bench --bench compare` remains the primary comparison for Lin, SQLite,
DuckDB, PostgreSQL and MySQL. Read queries use warm fixtures and prepared APIs.
Single-row update/delete now prepare queries outside timing, validate affected
rows and read-back before/after measurement, and cover 1k/10k/100k rows.
The previous update/delete report included Lin parsing, so it must not be used
as the baseline for this revised execution-only contract.

Airbug's `bench_with_input` and `bench_checked` always exclude input setup and
input destruction. `DropPolicy` controls the returned output's destruction only.
Use `--max-iterations 1 --warmup-ms 0` for expensive fresh-input workloads; the
reported values will then be individual timed operations, with less averaging.

```sh
cargo bench --bench compare -- --filter 1row --samples 8 --max-iterations 1 --warmup-ms 0 --output .airbug-bench/writes
cargo bench --bench compare -- --filter plan_dnf --samples 8 --max-iterations 1 --warmup-ms 0 --output .airbug-bench/planner
```

Existing SQL insert loops are API comparisons. They do not represent optimal
COPY, appender, or batched server ingestion. Lin's embedding and FTS maintenance
also do additional work. Add native bulk ingestion comparisons before claiming
that Lin beats a peer's best ingestion path.

## Extended read API comparison

`scripts/bench-peers.py` adds MongoDB, pandas, SQL Server and Kusto and can also
run SQLite/DuckDB/PostgreSQL/MySQL. `examples/peer_bench.rs` is a persistent Lin
worker, built in release mode. Each engine receives the same deterministic rows.
All results are checked against independently generated expected columns, values,
and multiplicities before/after measurement; row order is not compared.

Cases: point get returning id/title, indexed equality count, substring count,
filtered id/title materialization, inner FK join, and filtered inner FK join.
There is no implicit take limit. Joins use normal inner semantics, not `innerunique`.

Lin uses its prepared Rust row API. Python adapters measure driver/DBAPI and
DataFrame row APIs, including their language dispatch and result materialization.
DuckDB uses PREPARE/EXECUTE; PostgreSQL uses psycopg prepared execution; SQLite
uses its statement cache. PyMySQL/SQL Server/Kusto include their driver's query
submission overhead. These are API measurements, not isolated engine CPU costs.
Server measurements include network round-trip and result transfer. Pandas is
in-memory and makes no durability claim. Kusto query result caching is disabled.

JSON IPC, setup, validation, connection creation, and teardown are outside timers.
Each process/connection repetition is aggregated first, then medians are compared.
Batch-average percentiles are not individual-operation latency percentiles.
Do not measure while compilation, ingestion, or unrelated benchmark work runs.

```sh
python3 -m venv .bench-venv
.bench-venv/bin/python -m pip install -r scripts/bench-peers-requirements.lock
cargo build --release --example peer_bench
.bench-venv/bin/python scripts/bench-peers.py \
  --engines lin sqlite duckdb postgres mysql mongo pandas \
  --rows 10000 --samples 8 --repeats 3 --require-wins \
  --output .airbug-bench/peers-10k
```

Use 100k and 1M datasets, varied selectivity and skew, independent process
repetitions and native ingestion APIs before broadening the conclusion.
`--require-wins` fails if any measured peer beats Lin; unavailable/error cases
always make the run incomplete and return a nonzero exit code.

## Server scope

Only use dedicated benchmark instances/databases. SQL tables and Mongo databases
are uniquely named `linbench_<random>` and only those objects are removed.
Existing application collections/tables are never intentionally reused.

Connection configuration (secrets stay in environment variables):

- `LIN_BENCH_PG_URL`: default local benchmark Postgres on port 55432.
- `LIN_BENCH_MYSQL_URL`: default local benchmark MySQL on port 53306.
- `LIN_BENCH_MONGO_URL`: default local benchmark Mongo on port 27027.
- `LIN_BENCH_MSSQL_HOST`, `PORT` (default 1433), `USER`, `PASSWORD`, `DATABASE`.
- `LIN_BENCH_KUSTO_URL`, `DATABASE`, optional `TOKEN` (bearer access token).

SQL Server and Kusto adapters need live validation against their test instances.
An x86-64 emulator running on an ARM host must be labelled separately and must
not be used to claim a win against native x86-64 or cloud deployments.

CI runs fresh-fixture mutation cases separately with one timed operation per sample.
They are excluded from automatic calibration because untimed 100k-row fixture
construction can otherwise make calibration take excessively long.

## Native MongoDB insertion

Run `scripts/bench-peers.py --engines lin mongo --cases insert_native --rows 1000
--samples 9 --repeats 3 --require-wins --output .airbug-bench/mongo-native` with
`--lin-binary` pointing to the release `peer_bench` example. Native insertion
cannot be mixed with read cases. Unsupported native adapters fail as incomplete.

Each sample uses a fresh fixture. Schema/index creation, preparation, exact
six-field readback, cleanup and Lin IPC are excluded from timing. The timestamp
is shared by both engines within each process. Lin includes default embedding
and FTS. MongoDB uses ordered `insert_many`, maps `id` to `_id`, creates unique
URI and wing/ts indexes, and uses acknowledged writes (`w=1`, `j=false`). This
compares successful native API calls; it does not establish identical bulk
failure atomicity or physical disk durability. Mongo fixtures use unique owned
databases and cleanup only objects successfully created by this run.

### SQLite native insertion adapter

`--engines lin sqlite --cases insert_native` additionally compares Python sqlite3
`executemany` and commit in one transaction to the Lin prepared Rust call.
Each SQLite sample uses a fresh `:memory:` connection, unique id/uri and wing/ts
index. Six-field values and multiplicity are validated outside timing. Schema,
index setup and cursor creation are excluded; binding, insertion and commit are
included. Python overhead is included for SQLite; Lin IPC is excluded. Lin also
maintains default embedding and FTS. This API comparison does not replace the
native Rust benchmark or prove equal feature costs or disk durability.

### DuckDB native ingestion adapter

`--engines lin duckdb --cases insert_native` uses DuckDB INSERT SELECT from a
registered pandas DataFrame, not row-loop executemany. DataFrame construction,
registration and PREPARE of INSERT SELECT are outside timing. BEGIN, EXECUTE,
insertion and commit are timed; fresh schema/index setup, six-field readback
and close excluded. Unique id/uri and wing/ts index match the native fixture.
Lin uses prepared Rust execution with default embedding/FTS and excludes IPC;
these API and feature differences must remain explicit. No disk durability
or isolated engine CPU equivalence is claimed.

### PostgreSQL COPY insertion adapter

`--engines lin postgres --cases insert_native` uses Psycopg COPY FROM STDIN CSV
with a transaction commit. Prepared UTF-8 CSV, connection/cursor/schema/index
setup, exact readback and owned cleanup are outside timing. COPY transmission,
server work, acknowledgement and commit are included. The server's actual
synchronous_commit value is recorded; settings are not weakened. Every sample
uses an owned unique table, primary id, unique uri and wing/ts index. Only tables
successfully created by that sample are dropped. Server/WAL/network costs differ
from Lin Db::empty; tmpfs runs do not demonstrate physical-disk durability.

### MySQL native insertion adapter

`--engines lin mysql --cases insert_native` uses PyMySQL multi-row executemany
in one transaction per sample. BEGIN, escaping/SQL construction/transmission,
insertion and commit included; owned table/index/cursor/input setup, six-field
readback and cleanup excluded. Primary id/unique uri and wing/ts index use
bounded VARCHAR fields with utf8mb4; MySQL collation semantics differ from Lin.
Flush/binlog settings are recorded without weakening. Dedicated tmpfs server
results include driver/network/WAL costs and do not prove disk durability
or parity with Lin Db::empty. Cleanup rolls back and drops only created tables.

## Bounded Lin native insertion

The native insertion worker retains the single prepared call at <=10,000 rows.
Larger fixtures prepare statements of at most 10,000 rows outside timing, then
measure one `Db::empty` memory transaction containing all prepared calls and its
commit. Affected-row aggregation stays inside timing; exact six-field readback
and default embedding checks remain outside. Contract metadata records statement
count, row bound and actual execution path. No quota is raised: the previous
100k single-statement fixture exceeded the default 8 MiB AST limit and remains a
recorded incomplete result. This chunked memory invocation is a distinct API
contract and does not establish durable transaction atomicity or physical-disk
performance. It is compared with each peer's existing bulk ingestion adapter.


## No-change calibration for optimization claims

A separate SQLite control does not establish that changes in Lin timings are
caused by code. The [identical-binary control](../benches/results/2026-10-24-nochange-control/report.md)
showed material Lin durable 1k drift even while SQLite was comparatively stable.
For a modest durable optimization, calibrate the affected Lin workload with
same-source, identical-binary A/A pairs in comparable conditions, retain raw
observations and alternate order, then require independent before/after
repetition distinguishable from the observed no-change variation. A small
median difference overlapping that variation is inconclusive. The observed
sample is not a universal acceptance threshold or a confidence interval.

Keep rejected/noisy series and controls; do not select only a favorable repeat.
Do not infer a causal code regression merely from an unqualified negative value.
An A/A run from another time does not retroactively validate or invalidate
older source-specific results. Do not change sync/features/quotas or interfere
with unrelated host processes to manufacture stable wins. When attribution is
weak, improve isolation/protocol or target a larger measured effect before
retaining a production optimization.


## Microsoft peer execution environments

Kusto Emulator is for functional development/testing, not this performance
matrix: Microsoft explicitly prohibits emulator benchmark tests and describes a
different performance profile from Azure Data Explorer. It also excludes ARM
processors. Use a dedicated permitted service endpoint for Kusto timings.
SQL Server Linux containers are supported on Intel/AMD x86-64; an emulated ARM
host is a separate unsupported environment and cannot stand in for native peer
qualification. See the [checked sources and adapter regression](../benches/results/2026-10-24-microsoft-peer-qualification/report.md).
Missing endpoints remain missing coverage, never wins. Kusto cleanup attempts
all created tables even if one drop fails; any remaining failure still makes
the run incomplete rather than silently accepting leaked fixtures.


## Strict completion and numeric wins

Finalization derives medians from observations, requiring the expected distinct
process repetition IDs and nonempty finite positive samples. Invalid groups
cannot contribute medians; errors/missing groups yield incomplete status and
exit 2. With --require-wins, Lin must have a strictly smaller aggregate median
than every requested peer. A tie is complete evidence but fails that gate with
exit 3. Numeric superiority still requires the separate calibration and
independent-repeat evidence before claiming a causal optimization. The
[gate regression and saved-matrix revalidation](../benches/results/2026-10-24-strict-win-gate/report.md)
retain evidence; re-finalizing saved observations is not a fresh benchmark.


## Interleaved affected-engine control

The [persistent-worker durable A/A protocol](../benches/results/2026-10-24-interleaved-durable-control/report.md)
alternates individual checked fresh-fixture samples between two pinned workers,
rather than timing all samples of one executable before the other. Setup,
preparation, readback, IPC and drop/checkpoint are excluded; the timed prepared
run keeps Full sync/default embedding/FTS/indexes. It is an additional affected
Lin control, not a replacement for native/large-batch guards or peer timings.
Two A/A series show remaining noise; differences near that variation remain
inconclusive. Do not compare protocol changes as a causal engine speedup.


## SQL Server native insertion adapter

`--engines lin mssql --cases insert_native` now has a PyTDS typed bulk-copy
path. Each sample creates a fresh owned dbo table with primary id, unique uri
and wing/ts index outside timing. Explicit BEGIN, copy_to transfer/serialization
and COMMIT are timed; full six-field/multiplicity readback and cleanup are
excluded. NVARCHAR text and BIGINT timestamp metadata are explicit; constraints,
triggers and null preservation are enabled. Version, recovery model and delayed
durability are recorded without weakening settings. Only a successful CREATE
marks ownership; cleanup attempts rollback then DROP even on rollback failure.

This is native API coverage, not equal disk durability. SQL Server still
requires live read/typed bulk/transaction/cleanup qualification before any
performance result is accepted. The [implementation and mock/CLI evidence](../benches/results/2026-10-24-mssql-native-adapter/report.md)
do not replace a server run. No MSSQL win is claimed from adapter tests.


## Kusto native visible streaming ingestion

`--engines lin kusto --cases insert_native` has a bounded streaming REST path:
UTF-8 CSV input chunks prepared outside timing; all uploads/acknowledgements and
row-visibility polling timed; full six-field/multiplicity readback and owned
fixture setup/drop excluded. The 4MiB request cap is a conservative adapter
policy. Requests/visibility timeout and current streaming policy are recorded.
Cluster/database streaming must be preconfigured and inherited by fresh tables;
the adapter never changes those policies or silently falls back to queued ingest.
This API-specific case is not saturated ingestion throughput or SQL transaction,
uniqueness/index/durability equivalence.

Kusto .create table is idempotent even for an existing table; fixture ownership
therefore uses acknowledged .set with an empty typed datatable, which rejects an
existing name. This applies to read fixtures too. Live empty-schema creation,
streaming readiness, values and cleanup remain required before timing is valid.
[Implementation, official protocol references and mock/CLI gates](../benches/results/2026-10-24-kusto-streaming-adapter/report.md)
do not count as a live Kusto result or a win.


## Peer index constraints comparison

To ensure fair comparisons across peers (LIN-0008), the benchmark contracts enforce:
- **Lin**: B-Tree composite keys with unique/non-unique flags, inverted FTS postings with SIMD lowering, and HNSW vector graphs with cosine similarity.
- **SQLite**: B-Tree indices (`CREATE UNIQUE INDEX idx_docs_id ON docs(id); CREATE INDEX idx_docs_wing ON docs(wing, ts);`).
- **DuckDB**: ART (Adaptive Radix Tree) indices for point lookups and primary keys.
- **PostgreSQL**: B-Tree indices for primary and secondary keys (`btree`), GIN/GiST for full-text search.
- **MySQL**: B+Tree indices on InnoDB (`BTREE`), Fulltext index for text search.
- **MongoDB**: B-tree indices on `_id` and compound indexes on `(wing, ts)`.
- **SQL Server**: Clustered B-tree on primary key, nonclustered B-tree on secondary keys.
- **Kusto**: Inverted indexing of all columns with column-store partitioning.
- **pandas**: In-memory Hash index on index labels.

## Native SQL Server CI qualification

`.github/workflows/mssql-bench.yml` is manually dispatched on a dedicated Linux
x86-64 GitHub runner. `scripts/bench-mssql-native-ci.py` asserts the host and image
architectures, starts an owned SQL Server 2022 Developer container for nonproduction
testing with the documented EULA flag, generates temporary credentials, records
server settings/image digest/source and binary hashes, then runs read/native
1k/10k/100k comparisons with three processes and 12 samples. The script preserves
sentinel objects and removes only its owned container. It does not use ARM
emulation. Mere workflow creation is not a live peer result.

```sh
gh workflow run mssql-bench.yml --ref main
```

Raw reports are uploaded even on failure. Read/native API and server durability
differences above still apply; hosted-runner data must not be pooled with the
Apple-Silicon matrix or presented as a cloud-production latency guarantee.

SQL Server platform/container setup follows
[Microsoft's official quickstart](https://learn.microsoft.com/en-us/sql/linux/install-upgrade/quickstart-install-docker?view=sql-server-linux-ver16).

Kusto performance qualification requires a real test cluster. Microsoft's
[emulator overview](https://learn.microsoft.com/en-us/azure/data-explorer/kusto-emulator-overview)
explicitly excludes emulator benchmarking and describes a different performance
profile from the service. Separate emulator functional tests cannot count as
Kusto benchmark wins.

For a lightweight peer binary without root benchmark-only DuckDB/Airbug build
dependencies, use `cargo build --locked --release -p lin-peer-worker`; its source
is shared with the compatibility `peer_bench` example.
