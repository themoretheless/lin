# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.241 | 1.00× | validated |
| point_get | sqlite | 0.992 | 4.12× | validated |
| point_get | duckdb | 39.668 | 164.59× | validated |
| point_get | postgres | 271.038 | 1124.58× | validated |
| point_get | mysql | 346.728 | 1438.63× | validated |
| point_get | mongo | 517.305 | 2146.38× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.673 | 19.39× | validated |
| filter_eq | lin | 0.683 | 1.00× | validated |
| filter_eq | sqlite | 69.269 | 101.47× | validated |
| filter_eq | duckdb | 192.169 | 281.51× | validated |
| filter_eq | postgres | 495.923 | 726.49× | validated |
| filter_eq | mysql | 681.857 | 998.87× | validated |
| filter_eq | mongo | 932.552 | 1366.12× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 275.618 | 403.76× | validated |
| text_substr | lin | 17.382 | 1.00× | validated |
| text_substr | sqlite | 334.554 | 19.25× | validated |
| text_substr | duckdb | 104.490 | 6.01× | validated |
| text_substr | postgres | 789.260 | 45.41× | validated |
| text_substr | mysql | 1266.174 | 72.84× | validated |
| text_substr | mongo | 3056.229 | 175.83× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 506.891 | 29.16× | validated |
| materialize | lin | 421.597 | 1.00× | validated |
| materialize | sqlite | 1946.333 | 4.62× | validated |
| materialize | duckdb | 1266.812 | 3.00× | validated |
| materialize | postgres | 1694.677 | 4.02× | validated |
| materialize | mysql | 10057.834 | 23.86× | validated |
| materialize | mongo | 5859.083 | 13.90× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 1888.542 | 4.48× | validated |
| join_inner | lin | 1350.562 | 1.00× | validated |
| join_inner | sqlite | 4597.520 | 3.40× | validated |
| join_inner | duckdb | 2725.084 | 2.02× | validated |
| join_inner | postgres | 3834.291 | 2.84× | validated |
| join_inner | mysql | 21546.938 | 15.95× | validated |
| join_inner | mongo | 120274.895 | 89.06× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 4075.229 | 3.02× | validated |
| join_filter | lin | 663.809 | 1.00× | validated |
| join_filter | sqlite | 2344.354 | 3.53× | validated |
| join_filter | duckdb | 1482.681 | 2.23× | validated |
| join_filter | postgres | 2201.198 | 3.32× | validated |
| join_filter | mysql | 11498.834 | 17.32× | validated |
| join_filter | mongo | 61307.959 | 92.36× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 2332.010 | 3.51× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
