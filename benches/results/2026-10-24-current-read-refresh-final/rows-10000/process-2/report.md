# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.945 | 7.79× | validated |
| point_get | duckdb | 39.215 | 323.32× | validated |
| point_get | postgres | 172.975 | 1426.13× | validated |
| point_get | mysql | 331.441 | 2732.64× | validated |
| point_get | mongo | 528.558 | 4357.82× | validated |
| point_get | pandas | 4.525 | 37.31× | validated |
| point_get | lin | 0.121 | 1.00× | validated |
| filter_eq | sqlite | 73.268 | 112.67× | validated |
| filter_eq | duckdb | 184.256 | 283.34× | validated |
| filter_eq | postgres | 425.085 | 653.67× | validated |
| filter_eq | mysql | 637.490 | 980.29× | validated |
| filter_eq | mongo | 5182.458 | 7969.23× | validated |
| filter_eq | pandas | 253.662 | 390.06× | validated |
| filter_eq | lin | 0.650 | 1.00× | validated |
| text_substr | sqlite | 317.637 | 16.40× | validated |
| text_substr | duckdb | 97.168 | 5.02× | validated |
| text_substr | postgres | 670.309 | 34.61× | validated |
| text_substr | mysql | 1352.333 | 69.83× | validated |
| text_substr | mongo | 3621.229 | 186.98× | validated |
| text_substr | pandas | 487.040 | 25.15× | validated |
| text_substr | lin | 19.367 | 1.00× | validated |
| materialize | sqlite | 1819.073 | 5.03× | validated |
| materialize | duckdb | 1217.701 | 3.37× | validated |
| materialize | postgres | 1651.458 | 4.57× | validated |
| materialize | mysql | 12386.688 | 34.27× | validated |
| materialize | mongo | 5409.916 | 14.97× | validated |
| materialize | pandas | 1840.406 | 5.09× | validated |
| materialize | lin | 361.420 | 1.00× | validated |
| join_inner | sqlite | 4366.708 | 3.45× | validated |
| join_inner | duckdb | 2722.000 | 2.15× | validated |
| join_inner | postgres | 4051.354 | 3.20× | validated |
| join_inner | mysql | 21101.625 | 16.69× | validated |
| join_inner | mongo | 117658.583 | 93.07× | validated |
| join_inner | pandas | 4011.959 | 3.17× | validated |
| join_inner | lin | 1264.194 | 1.00× | validated |
| join_filter | sqlite | 2248.240 | 3.60× | validated |
| join_filter | duckdb | 1399.833 | 2.24× | validated |
| join_filter | postgres | 2557.688 | 4.10× | validated |
| join_filter | mysql | 11611.479 | 18.60× | validated |
| join_filter | mongo | 63430.896 | 101.59× | validated |
| join_filter | pandas | 2292.740 | 3.67× | validated |
| join_filter | lin | 624.375 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
