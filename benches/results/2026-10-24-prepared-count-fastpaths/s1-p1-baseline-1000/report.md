# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 27.542 | 1.00× | validated |
| filter_eq | sqlite | 7.633 | 0.28× | validated |
| filter_eq | duckdb | 151.999 | 5.52× | validated |
| filter_eq | pandas | 59.397 | 2.16× | validated |
| text_substr | lin | 28.525 | 1.00× | validated |
| text_substr | sqlite | 33.772 | 1.18× | validated |
| text_substr | duckdb | 75.240 | 2.64× | validated |
| text_substr | pandas | 77.935 | 2.73× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
