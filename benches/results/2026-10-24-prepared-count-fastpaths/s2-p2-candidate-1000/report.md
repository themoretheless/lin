# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.656 | 1.00× | validated |
| filter_eq | sqlite | 7.519 | 11.45× | validated |
| filter_eq | duckdb | 138.927 | 211.65× | validated |
| filter_eq | pandas | 57.506 | 87.61× | validated |
| text_substr | lin | 2.255 | 1.00× | validated |
| text_substr | sqlite | 31.773 | 14.09× | validated |
| text_substr | duckdb | 66.190 | 29.36× | validated |
| text_substr | pandas | 75.702 | 33.58× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
