# Validated read API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.115 | 1.00× | validated |
| point_get | sqlite | 0.929 | 8.07× | validated |
| point_get | duckdb | 40.549 | 352.25× | validated |
| point_get | postgres | 244.930 | 2127.67× | validated |
| point_get | mysql | 272.333 | 2365.71× | validated |
| point_get | mongo | 420.079 | 3649.15× | validated |
| point_get | pandas | 4.442 | 38.59× | validated |
| filter_eq | lin | 0.667 | 1.00× | validated |
| filter_eq | sqlite | 67.662 | 101.43× | validated |
| filter_eq | duckdb | 186.141 | 279.05× | validated |
| filter_eq | postgres | 465.146 | 697.31× | validated |
| filter_eq | mysql | 591.286 | 886.41× | validated |
| filter_eq | mongo | 906.029 | 1358.25× | validated |
| filter_eq | pandas | 267.843 | 401.53× | validated |
| text_substr | lin | 18.541 | 1.00× | validated |
| text_substr | sqlite | 324.044 | 17.48× | validated |
| text_substr | duckdb | 99.181 | 5.35× | validated |
| text_substr | postgres | 733.035 | 39.54× | validated |
| text_substr | mysql | 1260.125 | 67.96× | validated |
| text_substr | mongo | 3102.459 | 167.33× | validated |
| text_substr | pandas | 482.873 | 26.04× | validated |
| materialize | lin | 377.835 | 1.00× | validated |
| materialize | sqlite | 1846.146 | 4.89× | validated |
| materialize | duckdb | 1244.708 | 3.29× | validated |
| materialize | postgres | 1858.646 | 4.92× | validated |
| materialize | mysql | 10255.917 | 27.14× | validated |
| materialize | mongo | 5511.208 | 14.59× | validated |
| materialize | pandas | 1976.542 | 5.23× | validated |
| join_inner | lin | 1338.472 | 1.00× | validated |
| join_inner | sqlite | 4451.333 | 3.33× | validated |
| join_inner | duckdb | 2627.125 | 1.96× | validated |
| join_inner | postgres | 3925.521 | 2.93× | validated |
| join_inner | mysql | 20618.875 | 15.40× | validated |
| join_inner | mongo | 121065.312 | 90.45× | validated |
| join_inner | pandas | 3892.562 | 2.91× | validated |
| join_filter | lin | 654.351 | 1.00× | validated |
| join_filter | sqlite | 2343.635 | 3.58× | validated |
| join_filter | duckdb | 1405.604 | 2.15× | validated |
| join_filter | postgres | 2270.812 | 3.47× | validated |
| join_filter | mysql | 11159.124 | 17.05× | validated |
| join_filter | mongo | 63193.292 | 96.57× | validated |
| join_filter | pandas | 2242.917 | 3.43× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
