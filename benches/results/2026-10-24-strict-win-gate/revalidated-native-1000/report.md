# Validated native insert API benchmark

Rows: 1000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Fresh fixture per sample; schema/index/preparation, exact readback, drop and Lin JSON IPC excluded. Writes store the same six common fields.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| insert_native | lin | 742.250 | 1.00× | validated |
| insert_native | sqlite | 1008.354 | 1.36× | validated |
| insert_native | duckdb | 1149.729 | 1.55× | validated |
| insert_native | postgres | 4636.812 | 6.25× | validated |
| insert_native | mysql | 8261.333 | 11.13× | validated |
| insert_native | mongo | 4987.000 | 6.72× | validated |

Native contracts: {"duckdb": {"api": "DuckDB prepared INSERT SELECT from registered pandas DataFrame plus commit", "atomicity": "one transaction per sample", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "storage": ":memory:"}, "lin": {"api": "Rust prepared run; default embedding and FTS", "durability": "native memory API comparison; no disk durability equivalence", "execution_path": "prepared run", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "statement_rows_limit": 10000, "statements": 1, "storage": "Db::empty"}, "mongo": {"api": "PyMongo ordered insert_many; id maps to native _id", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "write_concern": {"j": false, "w": 1}}, "mysql": {"api": "PyMySQL executemany multi-row INSERT plus transaction commit", "atomicity": "one transaction per sample", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "innodb_flush_log_at_trx_commit": 1, "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "storage": "configured dedicated server", "sync_binlog": 1}, "postgres": {"api": "Psycopg COPY CSV block plus transaction commit", "atomicity": "one transaction per sample", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "storage": "configured dedicated server", "synchronous_commit": "on"}, "sqlite": {"api": "Python sqlite3 executemany plus transaction commit", "atomicity": "one transaction per sample", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "storage": ":memory:"}}

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
