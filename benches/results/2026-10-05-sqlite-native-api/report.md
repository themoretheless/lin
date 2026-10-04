# Native API insert: Lin vs Python SQLite executemany

Added a SQLite adapter to the existing insert_native workload. Lin production source is unchanged (retained classifier baseline); the pinned existing peer_bench binary comes from the current-peer-read-matrix source 588136e. Binary and source hashes retained. SQLite 3.53.4, Lin 0.4.0. Three independent process repetitions per size, 24 fresh fixtures per process/engine; exact six-field values and multiplicity checked outside timing. No build/test overlap. Both parent reports complete and require-wins exits zero.

| Rows | Lin median ms | SQLite median ms | SQLite / Lin | Process wins |
|---|---:|---:|---:|---:|
| 1000 | 1.233437 | 1.555438 | 1.261x | 3/3 |
| 10000 | 26.929792 | 38.801980 | 1.441x | 3/3 |

Aggregates are medians of process medians, not latency percentiles. Python SQLite executemany bindings/insertion and one transaction commit are timed; fresh :memory: schema, unique id/uri and wing/ts index setup, cursor creation, readback and close are excluded. Lin prepared Rust operation includes default embedding and FTS, excludes IPC/setup/readback/drop. Timestamp is shared within each process. SQLite adapter has no embedding/FTS equivalent. This is an API comparison, not isolated kernel CPU, equal feature work, or disk durability. It does not erase outstanding native Rust SQLite 1k and Full durable 1k losses or prove superiority over absent peers.

Eight benchmark-contract tests pass, including three fresh SQLite samples with exact timestamp and all fields. Existing unsupported-peer gate now checks pandas, which still has no native insertion adapter. PostgreSQL/MySQL/DuckDB/MSSQL/Kusto/pandas native adapters remain unsupported, explicitly incomplete rather than counted as wins. No production DB optimization ships in this change; comparison coverage is expanded.
