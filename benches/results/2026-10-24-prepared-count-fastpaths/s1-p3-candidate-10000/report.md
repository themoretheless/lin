# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.712 | 1.00× | validated |
| filter_eq | sqlite | 69.568 | 97.72× | validated |
| filter_eq | duckdb | 189.845 | 266.66× | validated |
| filter_eq | pandas | 267.125 | 375.21× | validated |
| text_substr | lin | 16.694 | 1.00× | validated |
| text_substr | sqlite | 316.545 | 18.96× | validated |
| text_substr | duckdb | 104.025 | 6.23× | validated |
| text_substr | pandas | 474.358 | 28.41× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
