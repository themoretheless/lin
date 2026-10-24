# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.112 | 1.00× | validated |
| point_get | sqlite | 0.978 | 8.73× | validated |
| point_get | duckdb | 39.578 | 353.22× | validated |
| point_get | postgres | 241.621 | 2156.37× | validated |
| point_get | mysql | 319.109 | 2847.92× | validated |
| point_get | mongo | 506.033 | 4516.13× | validated |
| point_get | pandas | 4.411 | 39.37× | validated |
| filter_eq | lin | 0.845 | 1.00× | validated |
| filter_eq | sqlite | 670.634 | 794.09× | validated |
| filter_eq | duckdb | 420.371 | 497.76× | validated |
| filter_eq | postgres | 2332.312 | 2761.67× | validated |
| filter_eq | mysql | 3326.604 | 3939.00× | validated |
| filter_eq | mongo | 5015.687 | 5939.03× | validated |
| filter_eq | pandas | 2225.052 | 2634.66× | validated |
| text_substr | lin | 207.469 | 1.00× | validated |
| text_substr | sqlite | 3442.291 | 16.59× | validated |
| text_substr | duckdb | 469.856 | 2.26× | validated |
| text_substr | postgres | 5027.167 | 24.23× | validated |
| text_substr | mysql | 9409.062 | 45.35× | validated |
| text_substr | mongo | 27405.896 | 132.10× | validated |
| text_substr | pandas | 4396.396 | 21.19× | validated |
| materialize | lin | 4988.105 | 1.00× | validated |
| materialize | sqlite | 20086.438 | 4.03× | validated |
| materialize | duckdb | 12343.083 | 2.47× | validated |
| materialize | postgres | 13982.354 | 2.80× | validated |
| materialize | mysql | 84207.980 | 16.88× | validated |
| materialize | mongo | 42705.312 | 8.56× | validated |
| materialize | pandas | 17108.730 | 3.43× | validated |
| join_inner | lin | 15733.062 | 1.00× | validated |
| join_inner | sqlite | 49733.062 | 3.16× | validated |
| join_inner | duckdb | 28571.229 | 1.82× | validated |
| join_inner | postgres | 39146.083 | 2.49× | validated |
| join_inner | mysql | 168532.958 | 10.71× | validated |
| join_inner | mongo | 1116295.646 | 70.95× | validated |
| join_inner | pandas | 35976.834 | 2.29× | validated |
| join_filter | lin | 7060.708 | 1.00× | validated |
| join_filter | sqlite | 24896.188 | 3.53× | validated |
| join_filter | duckdb | 14283.896 | 2.02× | validated |
| join_filter | postgres | 21015.146 | 2.98× | validated |
| join_filter | mysql | 90628.062 | 12.84× | validated |
| join_filter | mongo | 567341.708 | 80.35× | validated |
| join_filter | pandas | 18761.834 | 2.66× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
