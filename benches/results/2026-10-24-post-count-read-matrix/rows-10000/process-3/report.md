# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 41.332 | 379.58× | validated |
| point_get | pandas | 4.372 | 40.15× | validated |
| point_get | lin | 0.109 | 1.00× | validated |
| point_get | sqlite | 0.975 | 8.96× | validated |
| filter_eq | duckdb | 184.579 | 277.81× | validated |
| filter_eq | pandas | 263.512 | 396.62× | validated |
| filter_eq | lin | 0.664 | 1.00× | validated |
| filter_eq | sqlite | 72.461 | 109.06× | validated |
| text_substr | duckdb | 100.771 | 6.05× | validated |
| text_substr | pandas | 460.062 | 27.62× | validated |
| text_substr | lin | 16.654 | 1.00× | validated |
| text_substr | sqlite | 347.967 | 20.89× | validated |
| materialize | duckdb | 1293.674 | 3.26× | validated |
| materialize | pandas | 1902.011 | 4.80× | validated |
| materialize | lin | 396.226 | 1.00× | validated |
| materialize | sqlite | 1955.240 | 4.93× | validated |
| join_inner | duckdb | 2617.459 | 1.85× | validated |
| join_inner | pandas | 3986.938 | 2.82× | validated |
| join_inner | lin | 1415.812 | 1.00× | validated |
| join_inner | sqlite | 4735.917 | 3.35× | validated |
| join_filter | duckdb | 1440.708 | 2.08× | validated |
| join_filter | pandas | 2168.406 | 3.13× | validated |
| join_filter | lin | 691.941 | 1.00× | validated |
| join_filter | sqlite | 2460.521 | 3.56× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
