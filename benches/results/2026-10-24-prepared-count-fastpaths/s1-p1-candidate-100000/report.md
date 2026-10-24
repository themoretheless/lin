# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.867 | 1.00× | validated |
| filter_eq | sqlite | 1140.562 | 1315.15× | validated |
| filter_eq | duckdb | 442.780 | 510.56× | validated |
| filter_eq | pandas | 2288.594 | 2638.91× | validated |
| text_substr | lin | 210.793 | 1.00× | validated |
| text_substr | sqlite | 4209.208 | 19.97× | validated |
| text_substr | duckdb | 470.377 | 2.23× | validated |
| text_substr | pandas | 4750.958 | 22.54× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
