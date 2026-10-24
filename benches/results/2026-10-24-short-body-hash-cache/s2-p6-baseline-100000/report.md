# Validated native insert API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Fresh fixture per sample; schema/index/preparation, exact readback, drop and Lin JSON IPC excluded. Writes store the same six common fields.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| insert_native | lin | 106191.021 | 1.00× | validated |
| insert_native | sqlite | 143273.896 | 1.35× | validated |
| insert_native | duckdb | 100932.292 | 0.95× | validated |

Native contracts: {"duckdb": {"api": "DuckDB prepared INSERT SELECT from registered pandas DataFrame plus commit", "atomicity": "one transaction per sample", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "storage": ":memory:"}, "lin": {"api": "Rust prepared run; default embedding and FTS", "durability": "native memory API comparison; no disk durability equivalence", "execution_path": "prepared chunks in one memory transaction", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "statement_rows_limit": 10000, "statements": 10, "storage": "Db::empty"}, "sqlite": {"api": "Python sqlite3 executemany plus transaction commit", "atomicity": "one transaction per sample", "durability": "native memory API comparison; no disk durability equivalence", "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded", "schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index", "storage": ":memory:"}}

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
