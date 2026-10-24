# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.905 | 7.71× | validated |
| point_get | duckdb | 39.873 | 339.63× | validated |
| point_get | postgres | 291.251 | 2480.85× | validated |
| point_get | mysql | 267.460 | 2278.19× | validated |
| point_get | mongo | 416.058 | 3543.93× | validated |
| point_get | pandas | 5.098 | 43.42× | validated |
| point_get | lin | 0.117 | 1.00× | validated |
| filter_eq | sqlite | 7.411 | 11.58× | validated |
| filter_eq | duckdb | 134.075 | 209.44× | validated |
| filter_eq | postgres | 319.308 | 498.80× | validated |
| filter_eq | mysql | 290.751 | 454.19× | validated |
| filter_eq | mongo | 487.204 | 761.08× | validated |
| filter_eq | pandas | 55.386 | 86.52× | validated |
| filter_eq | lin | 0.640 | 1.00× | validated |
| text_substr | sqlite | 31.759 | 18.56× | validated |
| text_substr | duckdb | 58.319 | 34.08× | validated |
| text_substr | postgres | 334.551 | 195.52× | validated |
| text_substr | mysql | 354.726 | 207.31× | validated |
| text_substr | mongo | 707.077 | 413.24× | validated |
| text_substr | pandas | 75.435 | 44.09× | validated |
| text_substr | lin | 1.711 | 1.00× | validated |
| materialize | sqlite | 163.038 | 3.86× | validated |
| materialize | duckdb | 300.593 | 7.11× | validated |
| materialize | postgres | 397.169 | 9.40× | validated |
| materialize | mysql | 1173.443 | 27.76× | validated |
| materialize | mongo | 1232.660 | 29.16× | validated |
| materialize | pandas | 374.283 | 8.85× | validated |
| materialize | lin | 42.273 | 1.00× | validated |
| join_inner | sqlite | 381.378 | 2.95× | validated |
| join_inner | duckdb | 343.819 | 2.66× | validated |
| join_inner | postgres | 612.401 | 4.74× | validated |
| join_inner | mysql | 2518.625 | 19.48× | validated |
| join_inner | mongo | 13411.417 | 103.71× | validated |
| join_inner | pandas | 708.670 | 5.48× | validated |
| join_inner | lin | 129.316 | 1.00× | validated |
| join_filter | sqlite | 196.620 | 3.11× | validated |
| join_filter | duckdb | 242.497 | 3.83× | validated |
| join_filter | postgres | 441.049 | 6.97× | validated |
| join_filter | mysql | 1824.063 | 28.83× | validated |
| join_filter | mongo | 6942.896 | 109.74× | validated |
| join_filter | pandas | 626.723 | 9.91× | validated |
| join_filter | lin | 63.268 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
