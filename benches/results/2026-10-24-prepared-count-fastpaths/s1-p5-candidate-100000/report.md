# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.821 | 1.00× | validated |
| filter_eq | sqlite | 677.903 | 826.10× | validated |
| filter_eq | duckdb | 424.206 | 516.94× | validated |
| filter_eq | pandas | 2267.813 | 2763.57× | validated |
| text_substr | lin | 206.060 | 1.00× | validated |
| text_substr | sqlite | 3219.354 | 15.62× | validated |
| text_substr | duckdb | 455.231 | 2.21× | validated |
| text_substr | pandas | 4491.604 | 21.80× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
