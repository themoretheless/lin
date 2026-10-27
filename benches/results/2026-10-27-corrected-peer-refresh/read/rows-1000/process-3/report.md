# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 40.256 | 342.87× | validated |
| point_get | postgres | 314.042 | 2674.73× | validated |
| point_get | mysql | 329.547 | 2806.79× | validated |
| point_get | mongo | 419.440 | 3572.42× | validated |
| point_get | pandas | 4.409 | 37.55× | validated |
| point_get | lin | 0.117 | 1.00× | validated |
| point_get | sqlite | 0.919 | 7.82× | validated |
| filter_eq | duckdb | 195.079 | 291.44× | validated |
| filter_eq | postgres | 358.871 | 536.14× | validated |
| filter_eq | mysql | 378.943 | 566.13× | validated |
| filter_eq | mongo | 499.285 | 745.92× | validated |
| filter_eq | pandas | 55.993 | 83.65× | validated |
| filter_eq | lin | 0.669 | 1.00× | validated |
| filter_eq | sqlite | 7.572 | 11.31× | validated |
| text_substr | duckdb | 97.950 | 45.80× | validated |
| text_substr | postgres | 485.835 | 227.18× | validated |
| text_substr | mysql | 398.506 | 186.35× | validated |
| text_substr | mongo | 742.256 | 347.09× | validated |
| text_substr | pandas | 74.884 | 35.02× | validated |
| text_substr | lin | 2.139 | 1.00× | validated |
| text_substr | sqlite | 32.345 | 15.13× | validated |
| materialize | duckdb | 299.190 | 7.58× | validated |
| materialize | postgres | 551.053 | 13.97× | validated |
| materialize | mysql | 1239.901 | 31.43× | validated |
| materialize | mongo | 1652.583 | 41.89× | validated |
| materialize | pandas | 372.283 | 9.44× | validated |
| materialize | lin | 39.446 | 1.00× | validated |
| materialize | sqlite | 171.726 | 4.35× | validated |
| join_inner | duckdb | 399.467 | 3.08× | validated |
| join_inner | postgres | 695.490 | 5.36× | validated |
| join_inner | mysql | 2320.927 | 17.90× | validated |
| join_inner | mongo | 13141.209 | 101.37× | validated |
| join_inner | pandas | 718.441 | 5.54× | validated |
| join_inner | lin | 129.639 | 1.00× | validated |
| join_inner | sqlite | 399.210 | 3.08× | validated |
| join_filter | duckdb | 276.369 | 4.26× | validated |
| join_filter | postgres | 512.589 | 7.91× | validated |
| join_filter | mysql | 1440.174 | 22.21× | validated |
| join_filter | mongo | 7312.167 | 112.78× | validated |
| join_filter | pandas | 675.031 | 10.41× | validated |
| join_filter | lin | 64.833 | 1.00× | validated |
| join_filter | sqlite | 205.201 | 3.17× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
