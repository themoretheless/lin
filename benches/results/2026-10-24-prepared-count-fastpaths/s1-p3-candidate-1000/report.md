# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.654 | 1.00× | validated |
| filter_eq | sqlite | 7.513 | 11.48× | validated |
| filter_eq | duckdb | 141.801 | 216.74× | validated |
| filter_eq | pandas | 55.321 | 84.56× | validated |
| text_substr | lin | 1.703 | 1.00× | validated |
| text_substr | sqlite | 32.496 | 19.08× | validated |
| text_substr | duckdb | 59.508 | 34.94× | validated |
| text_substr | pandas | 76.219 | 44.75× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
