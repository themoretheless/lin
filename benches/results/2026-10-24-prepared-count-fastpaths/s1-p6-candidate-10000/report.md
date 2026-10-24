# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.675 | 1.00× | validated |
| filter_eq | sqlite | 67.623 | 100.17× | validated |
| filter_eq | duckdb | 177.710 | 263.24× | validated |
| filter_eq | pandas | 250.726 | 371.40× | validated |
| text_substr | lin | 16.644 | 1.00× | validated |
| text_substr | sqlite | 318.525 | 19.14× | validated |
| text_substr | duckdb | 93.260 | 5.60× | validated |
| text_substr | pandas | 466.294 | 28.02× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
