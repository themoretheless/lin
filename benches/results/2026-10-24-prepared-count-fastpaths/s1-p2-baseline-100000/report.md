# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 6091.355 | 1.00× | validated |
| filter_eq | sqlite | 690.116 | 0.11× | validated |
| filter_eq | duckdb | 499.494 | 0.08× | validated |
| filter_eq | pandas | 2299.729 | 0.38× | validated |
| text_substr | lin | 6327.812 | 1.00× | validated |
| text_substr | sqlite | 3417.146 | 0.54× | validated |
| text_substr | duckdb | 502.440 | 0.08× | validated |
| text_substr | pandas | 4756.562 | 0.75× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
