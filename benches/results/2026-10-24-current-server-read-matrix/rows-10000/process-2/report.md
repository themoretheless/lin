# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.921 | 7.14× | validated |
| point_get | duckdb | 37.454 | 290.37× | validated |
| point_get | postgres | 280.714 | 2176.25× | validated |
| point_get | mysql | 346.613 | 2687.14× | validated |
| point_get | mongo | 455.505 | 3531.33× | validated |
| point_get | pandas | 4.619 | 35.81× | validated |
| point_get | lin | 0.129 | 1.00× | validated |
| filter_eq | sqlite | 69.082 | 98.29× | validated |
| filter_eq | duckdb | 194.188 | 276.30× | validated |
| filter_eq | postgres | 488.081 | 694.47× | validated |
| filter_eq | mysql | 690.905 | 983.06× | validated |
| filter_eq | mongo | 1045.870 | 1488.13× | validated |
| filter_eq | pandas | 282.336 | 401.73× | validated |
| filter_eq | lin | 0.703 | 1.00× | validated |
| text_substr | sqlite | 328.552 | 20.22× | validated |
| text_substr | duckdb | 105.150 | 6.47× | validated |
| text_substr | postgres | 776.854 | 47.80× | validated |
| text_substr | mysql | 1306.820 | 80.41× | validated |
| text_substr | mongo | 3248.166 | 199.86× | validated |
| text_substr | pandas | 515.021 | 31.69× | validated |
| text_substr | lin | 16.252 | 1.00× | validated |
| materialize | sqlite | 1858.802 | 4.90× | validated |
| materialize | duckdb | 1247.111 | 3.29× | validated |
| materialize | postgres | 1911.833 | 5.04× | validated |
| materialize | mysql | 9827.209 | 25.91× | validated |
| materialize | mongo | 5588.312 | 14.74× | validated |
| materialize | pandas | 1986.458 | 5.24× | validated |
| materialize | lin | 379.247 | 1.00× | validated |
| join_inner | sqlite | 4457.188 | 3.27× | validated |
| join_inner | duckdb | 2643.874 | 1.94× | validated |
| join_inner | postgres | 4628.833 | 3.39× | validated |
| join_inner | mysql | 20745.583 | 15.20× | validated |
| join_inner | mongo | 115275.583 | 84.47× | validated |
| join_inner | pandas | 4004.500 | 2.93× | validated |
| join_inner | lin | 1364.632 | 1.00× | validated |
| join_filter | sqlite | 2327.250 | 3.43× | validated |
| join_filter | duckdb | 1395.229 | 2.06× | validated |
| join_filter | postgres | 2337.709 | 3.45× | validated |
| join_filter | mysql | 11080.625 | 16.34× | validated |
| join_filter | mongo | 59832.251 | 88.23× | validated |
| join_filter | pandas | 2373.302 | 3.50× | validated |
| join_filter | lin | 678.141 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
