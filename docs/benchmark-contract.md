# Benchmark contract

The performance target is a measured win for Lin on explicitly defined workloads
against SQLite, DuckDB, PostgreSQL, MySQL, MongoDB, SQL Server, Kusto, and pandas.
A missing engine, incorrect result, or incompatible workload is not a win.
This is not a claim of superiority for every workload or data size.

## Native Rust comparison

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
