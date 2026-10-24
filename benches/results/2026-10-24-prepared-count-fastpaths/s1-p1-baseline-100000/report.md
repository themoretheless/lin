# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 6337.979 | 1.00× | validated |
| filter_eq | sqlite | 688.533 | 0.11× | validated |
| filter_eq | duckdb | 437.110 | 0.07× | validated |
| filter_eq | pandas | 2343.760 | 0.37× | validated |
| text_substr | lin | 6448.979 | 1.00× | validated |
| text_substr | sqlite | 3537.396 | 0.55× | validated |
| text_substr | duckdb | 492.315 | 0.08× | validated |
| text_substr | pandas | 4856.355 | 0.75× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
