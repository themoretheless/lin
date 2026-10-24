# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 26.054 | 1.00× | validated |
| filter_eq | sqlite | 7.382 | 0.28× | validated |
| filter_eq | duckdb | 139.579 | 5.36× | validated |
| filter_eq | pandas | 55.353 | 2.12× | validated |
| text_substr | lin | 26.927 | 1.00× | validated |
| text_substr | sqlite | 31.345 | 1.16× | validated |
| text_substr | duckdb | 62.481 | 2.32× | validated |
| text_substr | pandas | 74.717 | 2.77× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
