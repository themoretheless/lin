# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 40.395 | 350.91× | validated |
| point_get | postgres | 250.083 | 2172.43× | validated |
| point_get | mysql | 272.333 | 2365.71× | validated |
| point_get | mongo | 453.818 | 3942.24× | validated |
| point_get | pandas | 4.558 | 39.60× | validated |
| point_get | lin | 0.115 | 1.00× | validated |
| point_get | sqlite | 0.929 | 8.07× | validated |
| filter_eq | duckdb | 186.141 | 279.05× | validated |
| filter_eq | postgres | 470.385 | 705.16× | validated |
| filter_eq | mysql | 591.286 | 886.41× | validated |
| filter_eq | mongo | 1033.921 | 1549.97× | validated |
| filter_eq | pandas | 266.427 | 399.41× | validated |
| filter_eq | lin | 0.667 | 1.00× | validated |
| filter_eq | sqlite | 67.662 | 101.43× | validated |
| text_substr | duckdb | 98.612 | 5.55× | validated |
| text_substr | postgres | 733.035 | 41.24× | validated |
| text_substr | mysql | 1279.985 | 72.02× | validated |
| text_substr | mongo | 3324.750 | 187.06× | validated |
| text_substr | pandas | 475.691 | 26.76× | validated |
| text_substr | lin | 17.774 | 1.00× | validated |
| text_substr | sqlite | 324.092 | 18.23× | validated |
| materialize | duckdb | 1244.109 | 3.34× | validated |
| materialize | postgres | 1888.333 | 5.06× | validated |
| materialize | mysql | 10427.124 | 27.96× | validated |
| materialize | mongo | 5511.208 | 14.78× | validated |
| materialize | pandas | 1976.542 | 5.30× | validated |
| materialize | lin | 372.954 | 1.00× | validated |
| materialize | sqlite | 1927.667 | 5.17× | validated |
| join_inner | duckdb | 2627.125 | 1.92× | validated |
| join_inner | postgres | 3866.833 | 2.83× | validated |
| join_inner | mysql | 20858.855 | 15.27× | validated |
| join_inner | mongo | 123920.230 | 90.71× | validated |
| join_inner | pandas | 3883.479 | 2.84× | validated |
| join_inner | lin | 1366.097 | 1.00× | validated |
| join_inner | sqlite | 4599.938 | 3.37× | validated |
| join_filter | duckdb | 1402.070 | 2.14× | validated |
| join_filter | postgres | 2329.365 | 3.56× | validated |
| join_filter | mysql | 11341.833 | 17.32× | validated |
| join_filter | mongo | 64338.104 | 98.23× | validated |
| join_filter | pandas | 2235.823 | 3.41× | validated |
| join_filter | lin | 654.976 | 1.00× | validated |
| join_filter | sqlite | 2361.438 | 3.61× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
