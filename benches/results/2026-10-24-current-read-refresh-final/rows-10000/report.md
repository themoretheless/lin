# Validated read API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.121 | 1.00× | validated |
| point_get | sqlite | 0.945 | 7.79× | validated |
| point_get | duckdb | 39.215 | 323.32× | validated |
| point_get | postgres | 184.302 | 1519.52× | validated |
| point_get | mysql | 278.134 | 2293.14× | validated |
| point_get | mongo | 420.302 | 3465.28× | validated |
| point_get | pandas | 4.525 | 37.31× | validated |
| filter_eq | lin | 0.650 | 1.00× | validated |
| filter_eq | sqlite | 67.707 | 104.12× | validated |
| filter_eq | duckdb | 184.256 | 283.34× | validated |
| filter_eq | postgres | 425.085 | 653.67× | validated |
| filter_eq | mysql | 637.490 | 980.29× | validated |
| filter_eq | mongo | 892.604 | 1372.58× | validated |
| filter_eq | pandas | 259.657 | 399.28× | validated |
| text_substr | lin | 16.104 | 1.00× | validated |
| text_substr | sqlite | 317.637 | 19.72× | validated |
| text_substr | duckdb | 97.168 | 6.03× | validated |
| text_substr | postgres | 670.309 | 41.62× | validated |
| text_substr | mysql | 1277.826 | 79.35× | validated |
| text_substr | mongo | 3209.146 | 199.28× | validated |
| text_substr | pandas | 475.817 | 29.55× | validated |
| materialize | lin | 361.420 | 1.00× | validated |
| materialize | sqlite | 1872.323 | 5.18× | validated |
| materialize | duckdb | 1266.945 | 3.51× | validated |
| materialize | postgres | 1680.465 | 4.65× | validated |
| materialize | mysql | 9793.583 | 27.10× | validated |
| materialize | mongo | 5061.854 | 14.01× | validated |
| materialize | pandas | 1840.406 | 5.09× | validated |
| join_inner | lin | 1264.194 | 1.00× | validated |
| join_inner | sqlite | 4450.292 | 3.52× | validated |
| join_inner | duckdb | 2674.084 | 2.12× | validated |
| join_inner | postgres | 4051.354 | 3.20× | validated |
| join_inner | mysql | 21034.145 | 16.64× | validated |
| join_inner | mongo | 117658.583 | 93.07× | validated |
| join_inner | pandas | 3934.979 | 3.11× | validated |
| join_filter | lin | 628.676 | 1.00× | validated |
| join_filter | sqlite | 2254.521 | 3.59× | validated |
| join_filter | duckdb | 1399.833 | 2.23× | validated |
| join_filter | postgres | 2557.688 | 4.07× | validated |
| join_filter | mysql | 10762.396 | 17.12× | validated |
| join_filter | mongo | 62663.854 | 99.68× | validated |
| join_filter | pandas | 2292.740 | 3.65× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
