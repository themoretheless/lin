# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.113 | 1.00× | validated |
| point_get | sqlite | 0.887 | 7.81× | validated |
| point_get | duckdb | 33.671 | 296.82× | validated |
| point_get | postgres | 246.615 | 2173.98× | validated |
| point_get | mysql | 196.081 | 1728.50× | validated |
| point_get | mongo | 291.000 | 2565.24× | validated |
| point_get | pandas | 4.410 | 38.88× | validated |
| filter_eq | lin | 0.637 | 1.00× | validated |
| filter_eq | sqlite | 7.515 | 11.79× | validated |
| filter_eq | duckdb | 123.268 | 193.41× | validated |
| filter_eq | postgres | 207.808 | 326.05× | validated |
| filter_eq | mysql | 227.358 | 356.72× | validated |
| filter_eq | mongo | 383.957 | 602.42× | validated |
| filter_eq | pandas | 55.497 | 87.07× | validated |
| text_substr | lin | 1.840 | 1.00× | validated |
| text_substr | sqlite | 31.220 | 16.97× | validated |
| text_substr | duckdb | 51.355 | 27.91× | validated |
| text_substr | postgres | 221.394 | 120.32× | validated |
| text_substr | mysql | 277.424 | 150.77× | validated |
| text_substr | mongo | 651.930 | 354.30× | validated |
| text_substr | pandas | 75.279 | 40.91× | validated |
| materialize | lin | 39.001 | 1.00× | validated |
| materialize | sqlite | 161.610 | 4.14× | validated |
| materialize | duckdb | 289.848 | 7.43× | validated |
| materialize | postgres | 324.450 | 8.32× | validated |
| materialize | mysql | 1111.573 | 28.50× | validated |
| materialize | mongo | 1276.812 | 32.74× | validated |
| materialize | pandas | 390.296 | 10.01× | validated |
| join_inner | lin | 127.575 | 1.00× | validated |
| join_inner | sqlite | 385.061 | 3.02× | validated |
| join_inner | duckdb | 350.154 | 2.74× | validated |
| join_inner | postgres | 577.018 | 4.52× | validated |
| join_inner | mysql | 2119.938 | 16.62× | validated |
| join_inner | mongo | 13593.479 | 106.55× | validated |
| join_inner | pandas | 713.333 | 5.59× | validated |
| join_filter | lin | 67.592 | 1.00× | validated |
| join_filter | sqlite | 197.637 | 2.92× | validated |
| join_filter | duckdb | 240.146 | 3.55× | validated |
| join_filter | postgres | 396.740 | 5.87× | validated |
| join_filter | mysql | 1228.516 | 18.18× | validated |
| join_filter | mongo | 7001.292 | 103.58× | validated |
| join_filter | pandas | 661.899 | 9.79× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
