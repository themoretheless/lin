# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 5.569 | 7.94× | validated |
| point_get | duckdb | 127.028 | 181.22× | validated |
| point_get | postgres | 583.919 | 833.04× | validated |
| point_get | mysql | 842.422 | 1201.83× | validated |
| point_get | mongo | 3957.541 | 5645.97× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 12.188 | 17.39× | validated |
| point_get | lin | 0.701 | 1.00× | validated |
| filter_eq | sqlite | 233.104 | 155.08× | validated |
| filter_eq | duckdb | 464.409 | 308.96× | validated |
| filter_eq | postgres | 1186.224 | 789.16× | validated |
| filter_eq | mysql | 1798.833 | 1196.71× | validated |
| filter_eq | mongo | 11081.916 | 7372.50× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 821.092 | 546.25× | validated |
| filter_eq | lin | 1.503 | 1.00× | validated |
| text_substr | sqlite | 1834.892 | 34.57× | validated |
| text_substr | duckdb | 275.675 | 5.19× | validated |
| text_substr | postgres | 1842.532 | 34.72× | validated |
| text_substr | mysql | 3565.041 | 67.18× | validated |
| text_substr | mongo | 11553.604 | 217.70× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 2054.257 | 38.71× | validated |
| text_substr | lin | 53.070 | 1.00× | validated |
| materialize | sqlite | 4777.000 | 2.93× | validated |
| materialize | duckdb | 3043.771 | 1.87× | validated |
| materialize | postgres | 4654.562 | 2.85× | validated |
| materialize | mysql | 27455.521 | 16.84× | validated |
| materialize | mongo | 22651.084 | 13.89× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 5220.875 | 3.20× | validated |
| materialize | lin | 1630.729 | 1.00× | validated |
| join_inner | sqlite | 27824.750 | 4.18× | validated |
| join_inner | duckdb | 13071.229 | 1.96× | validated |
| join_inner | postgres | 11482.374 | 1.73× | validated |
| join_inner | mysql | 80842.438 | 12.15× | validated |
| join_inner | mongo | 332934.354 | 50.04× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 13230.251 | 1.99× | validated |
| join_inner | lin | 6653.250 | 1.00× | validated |
| join_filter | sqlite | 9193.751 | 4.85× | validated |
| join_filter | duckdb | 6086.270 | 3.21× | validated |
| join_filter | postgres | 7695.500 | 4.06× | validated |
| join_filter | mysql | 54206.626 | 28.62× | validated |
| join_filter | mongo | 240647.750 | 127.06× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 7841.646 | 4.14× | validated |
| join_filter | lin | 1894.031 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
