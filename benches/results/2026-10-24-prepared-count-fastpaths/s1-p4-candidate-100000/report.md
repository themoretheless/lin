# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.813 | 1.00× | validated |
| filter_eq | sqlite | 650.298 | 800.01× | validated |
| filter_eq | duckdb | 438.093 | 538.95× | validated |
| filter_eq | pandas | 2278.635 | 2803.21× | validated |
| text_substr | lin | 206.150 | 1.00× | validated |
| text_substr | sqlite | 3277.583 | 15.90× | validated |
| text_substr | duckdb | 468.381 | 2.27× | validated |
| text_substr | pandas | 4568.333 | 22.16× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
