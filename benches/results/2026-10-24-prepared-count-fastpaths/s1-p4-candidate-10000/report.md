# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.661 | 1.00× | validated |
| filter_eq | sqlite | 67.854 | 102.64× | validated |
| filter_eq | duckdb | 171.217 | 259.00× | validated |
| filter_eq | pandas | 255.067 | 385.84× | validated |
| text_substr | lin | 15.892 | 1.00× | validated |
| text_substr | sqlite | 318.823 | 20.06× | validated |
| text_substr | duckdb | 93.578 | 5.89× | validated |
| text_substr | pandas | 457.750 | 28.80× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
