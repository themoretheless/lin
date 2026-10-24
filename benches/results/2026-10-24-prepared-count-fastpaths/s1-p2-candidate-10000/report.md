# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.698 | 1.00× | validated |
| filter_eq | sqlite | 68.995 | 98.86× | validated |
| filter_eq | duckdb | 184.741 | 264.71× | validated |
| filter_eq | pandas | 257.860 | 369.48× | validated |
| text_substr | lin | 16.246 | 1.00× | validated |
| text_substr | sqlite | 328.126 | 20.20× | validated |
| text_substr | duckdb | 101.965 | 6.28× | validated |
| text_substr | pandas | 473.877 | 29.17× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
