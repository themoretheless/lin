# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.111 | 1.00× | validated |
| point_get | sqlite | 0.964 | 8.65× | validated |
| point_get | duckdb | 73.048 | 656.11× | validated |
| point_get | postgres | 241.621 | 2170.21× | validated |
| point_get | mysql | 314.944 | 2828.78× | validated |
| point_get | mongo | 575.107 | 5165.54× | validated |
| point_get | pandas | 4.481 | 40.25× | validated |
| filter_eq | lin | 0.836 | 1.00× | validated |
| filter_eq | sqlite | 647.366 | 774.68× | validated |
| filter_eq | duckdb | 449.008 | 537.31× | validated |
| filter_eq | postgres | 2332.312 | 2791.01× | validated |
| filter_eq | mysql | 3316.896 | 3969.23× | validated |
| filter_eq | mongo | 5015.687 | 6002.12× | validated |
| filter_eq | pandas | 2322.021 | 2778.69× | validated |
| text_substr | lin | 207.084 | 1.00× | validated |
| text_substr | sqlite | 3442.291 | 16.62× | validated |
| text_substr | duckdb | 476.692 | 2.30× | validated |
| text_substr | postgres | 5027.167 | 24.28× | validated |
| text_substr | mysql | 9409.062 | 45.44× | validated |
| text_substr | mongo | 26739.959 | 129.13× | validated |
| text_substr | pandas | 4396.396 | 21.23× | validated |
| materialize | lin | 4600.167 | 1.00× | validated |
| materialize | sqlite | 19389.083 | 4.21× | validated |
| materialize | duckdb | 12697.062 | 2.76× | validated |
| materialize | postgres | 13982.354 | 3.04× | validated |
| materialize | mysql | 84207.980 | 18.31× | validated |
| materialize | mongo | 42652.438 | 9.27× | validated |
| materialize | pandas | 17314.875 | 3.76× | validated |
| join_inner | lin | 15699.001 | 1.00× | validated |
| join_inner | sqlite | 49733.062 | 3.17× | validated |
| join_inner | duckdb | 28571.229 | 1.82× | validated |
| join_inner | postgres | 39146.083 | 2.49× | validated |
| join_inner | mysql | 168740.416 | 10.75× | validated |
| join_inner | mongo | 1125007.354 | 71.66× | validated |
| join_inner | pandas | 48333.250 | 3.08× | validated |
| join_filter | lin | 7516.000 | 1.00× | validated |
| join_filter | sqlite | 24224.458 | 3.22× | validated |
| join_filter | duckdb | 15436.103 | 2.05× | validated |
| join_filter | postgres | 20873.667 | 2.78× | validated |
| join_filter | mysql | 90628.062 | 12.06× | validated |
| join_filter | mongo | 559083.021 | 74.39× | validated |
| join_filter | pandas | 19813.375 | 2.64× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
