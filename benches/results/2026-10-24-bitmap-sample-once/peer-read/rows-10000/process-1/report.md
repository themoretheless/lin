# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.119 | 1.00× | validated |
| point_get | sqlite | 0.908 | 7.63× | validated |
| point_get | duckdb | 51.468 | 432.34× | validated |
| point_get | postgres | 257.488 | 2162.93× | validated |
| point_get | mysql | 275.946 | 2317.98× | validated |
| point_get | mongo | 418.530 | 3515.71× | validated |
| point_get | pandas | 4.526 | 38.02× | validated |
| filter_eq | lin | 0.691 | 1.00× | validated |
| filter_eq | sqlite | 68.977 | 99.89× | validated |
| filter_eq | duckdb | 201.835 | 292.30× | validated |
| filter_eq | postgres | 472.458 | 684.21× | validated |
| filter_eq | mysql | 629.365 | 911.44× | validated |
| filter_eq | mongo | 987.083 | 1429.49× | validated |
| filter_eq | pandas | 272.955 | 395.29× | validated |
| text_substr | lin | 16.690 | 1.00× | validated |
| text_substr | sqlite | 338.336 | 20.27× | validated |
| text_substr | duckdb | 109.314 | 6.55× | validated |
| text_substr | postgres | 805.212 | 48.24× | validated |
| text_substr | mysql | 1281.542 | 76.78× | validated |
| text_substr | mongo | 3156.354 | 189.11× | validated |
| text_substr | pandas | 473.035 | 28.34× | validated |
| materialize | lin | 388.441 | 1.00× | validated |
| materialize | sqlite | 1867.146 | 4.81× | validated |
| materialize | duckdb | 1398.271 | 3.60× | validated |
| materialize | postgres | 1642.090 | 4.23× | validated |
| materialize | mysql | 9732.771 | 25.06× | validated |
| materialize | mongo | 5294.833 | 13.63× | validated |
| materialize | pandas | 2134.416 | 5.49× | validated |
| join_inner | lin | 1389.340 | 1.00× | validated |
| join_inner | sqlite | 4476.792 | 3.22× | validated |
| join_inner | duckdb | 2894.021 | 2.08× | validated |
| join_inner | postgres | 4505.374 | 3.24× | validated |
| join_inner | mysql | 20454.480 | 14.72× | validated |
| join_inner | mongo | 115260.500 | 82.96× | validated |
| join_inner | pandas | 4285.812 | 3.08× | validated |
| join_filter | lin | 666.696 | 1.00× | validated |
| join_filter | sqlite | 2299.677 | 3.45× | validated |
| join_filter | duckdb | 1543.472 | 2.32× | validated |
| join_filter | postgres | 2228.021 | 3.34× | validated |
| join_filter | mysql | 10808.834 | 16.21× | validated |
| join_filter | mongo | 58200.438 | 87.30× | validated |
| join_filter | pandas | 2430.083 | 3.64× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
