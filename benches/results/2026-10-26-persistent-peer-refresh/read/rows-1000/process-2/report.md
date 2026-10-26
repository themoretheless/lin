# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.936 | 8.61× | validated |
| point_get | duckdb | 41.091 | 377.93× | validated |
| point_get | postgres | 315.836 | 2904.85× | validated |
| point_get | mysql | 298.567 | 2746.02× | validated |
| point_get | mongo | 458.597 | 4217.88× | validated |
| point_get | pandas | 4.456 | 40.98× | validated |
| point_get | lin | 0.109 | 1.00× | validated |
| filter_eq | sqlite | 7.449 | 11.14× | validated |
| filter_eq | duckdb | 143.097 | 214.00× | validated |
| filter_eq | postgres | 344.598 | 515.35× | validated |
| filter_eq | mysql | 365.534 | 546.66× | validated |
| filter_eq | mongo | 559.622 | 836.92× | validated |
| filter_eq | pandas | 57.932 | 86.64× | validated |
| filter_eq | lin | 0.669 | 1.00× | validated |
| text_substr | sqlite | 32.676 | 17.31× | validated |
| text_substr | duckdb | 65.744 | 34.82× | validated |
| text_substr | postgres | 362.075 | 191.79× | validated |
| text_substr | mysql | 429.504 | 227.50× | validated |
| text_substr | mongo | 768.462 | 407.04× | validated |
| text_substr | pandas | 79.444 | 42.08× | validated |
| text_substr | lin | 1.888 | 1.00× | validated |
| materialize | sqlite | 169.495 | 4.33× | validated |
| materialize | duckdb | 302.814 | 7.74× | validated |
| materialize | postgres | 435.828 | 11.13× | validated |
| materialize | mysql | 1335.007 | 34.10× | validated |
| materialize | mongo | 1325.995 | 33.87× | validated |
| materialize | pandas | 394.600 | 10.08× | validated |
| materialize | lin | 39.147 | 1.00× | validated |
| join_inner | sqlite | 398.714 | 3.03× | validated |
| join_inner | duckdb | 380.973 | 2.90× | validated |
| join_inner | postgres | 627.382 | 4.77× | validated |
| join_inner | mysql | 2384.094 | 18.12× | validated |
| join_inner | mongo | 12956.000 | 98.48× | validated |
| join_inner | pandas | 776.003 | 5.90× | validated |
| join_inner | lin | 131.559 | 1.00× | validated |
| join_filter | sqlite | 208.148 | 3.02× | validated |
| join_filter | duckdb | 267.694 | 3.89× | validated |
| join_filter | postgres | 510.729 | 7.41× | validated |
| join_filter | mysql | 1467.312 | 21.30× | validated |
| join_filter | mongo | 7839.688 | 113.82× | validated |
| join_filter | pandas | 711.274 | 10.33× | validated |
| join_filter | lin | 68.878 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
