# DuckDB bulk native API insertion

Source: 9d6c877 plus new DuckDB native adapter. Lin production unchanged, pinned peer_bench worker from source 588136e; hashes recorded. DuckDB 1.5.6 and pandas 3.0.6 input frame, Lin 0.4.0. Three independent processes per size, 24 fresh samples each. Complete reports, no errors, require-wins exit zero. Nine contract tests pass including three independent fresh DuckDB fixtures with exact six-field/timestamp checks. No builds/tests overlap timing.

| Rows | Lin median ms | DuckDB median ms | DuckDB / Lin | Process wins |
|---|---:|---:|---:|---:|
| 1000 | 1.408729 | 3.535166 | 2.509x | 3/3 |
| 10000 | 13.594250 | 17.333021 | 1.275x | 3/3 |

DuckDB uses a prepared pandas input frame registered outside timing, then BEGIN TRANSACTION, INSERT SELECT and commit inside timing. Each sample creates a fresh :memory: DB, primary id, unique uri, wing/ts index. Input preparation, registration/schema/index setup, exact readback and close excluded. SQL execute/binding/planning included for DuckDB; Lin operation is prepared and IPC excluded. Lin includes default embedding and FTS, DuckDB does not. This is the implemented API comparison, not equal feature costs, isolated engine CPU, physical disk durability or a claim about every API. The PostgreSQL/MySQL/MSSQL/Kusto/pandas native adapters remain unimplemented. Rust SQLite 1k and Full durable 1k gaps remain open. This change expands measurement coverage, not production DB speed.

Official ingestion route: https://www.duckdb.org/docs/current/clients/python/data_ingestion . DuckDB documents registered DataFrame INSERT SELECT, and advises against executemany for large ingestion in https://www.duckdb.org/docs/current/clients/python/dbapi .
