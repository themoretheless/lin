# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 5615.312 | 1.00× | validated |
| filter_eq | sqlite | 708.194 | 0.13× | validated |
| filter_eq | duckdb | 432.475 | 0.08× | validated |
| filter_eq | pandas | 2229.562 | 0.40× | validated |
| text_substr | lin | 6078.542 | 1.00× | validated |
| text_substr | sqlite | 3456.416 | 0.57× | validated |
| text_substr | duckdb | 466.906 | 0.08× | validated |
| text_substr | pandas | 4573.771 | 0.75× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
