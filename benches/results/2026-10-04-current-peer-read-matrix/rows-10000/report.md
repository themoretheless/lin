# Validated read API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.701 | 1.00× | validated |
| point_get | sqlite | 2.128 | 3.04× | validated |
| point_get | duckdb | 127.028 | 181.22× | validated |
| point_get | postgres | 583.919 | 833.04× | validated |
| point_get | mysql | 842.422 | 1201.83× | validated |
| point_get | mongo | 953.879 | 1360.84× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 12.188 | 17.39× | validated |
| filter_eq | lin | 1.503 | 1.00× | validated |
| filter_eq | sqlite | 218.172 | 145.14× | validated |
| filter_eq | duckdb | 464.409 | 308.96× | validated |
| filter_eq | postgres | 1186.224 | 789.16× | validated |
| filter_eq | mysql | 1661.750 | 1105.52× | validated |
| filter_eq | mongo | 1830.344 | 1217.68× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 689.196 | 458.50× | validated |
| text_substr | lin | 53.070 | 1.00× | validated |
| text_substr | sqlite | 737.746 | 13.90× | validated |
| text_substr | duckdb | 275.675 | 5.19× | validated |
| text_substr | postgres | 1842.532 | 34.72× | validated |
| text_substr | mysql | 2937.896 | 55.36× | validated |
| text_substr | mongo | 6574.896 | 123.89× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 1550.722 | 29.22× | validated |
| materialize | lin | 1574.296 | 1.00× | validated |
| materialize | sqlite | 4777.000 | 3.03× | validated |
| materialize | duckdb | 3043.771 | 1.93× | validated |
| materialize | postgres | 4654.562 | 2.96× | validated |
| materialize | mysql | 22031.604 | 13.99× | validated |
| materialize | mongo | 11795.396 | 7.49× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 5220.875 | 3.32× | validated |
| join_inner | lin | 4118.646 | 1.00× | validated |
| join_inner | sqlite | 13139.104 | 3.19× | validated |
| join_inner | duckdb | 7233.105 | 1.76× | validated |
| join_inner | postgres | 11482.374 | 2.79× | validated |
| join_inner | mysql | 47338.062 | 11.49× | validated |
| join_inner | mongo | 273561.230 | 66.42× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 13230.251 | 3.21× | validated |
| join_filter | lin | 1582.875 | 1.00× | validated |
| join_filter | sqlite | 6576.562 | 4.15× | validated |
| join_filter | duckdb | 3919.896 | 2.48× | validated |
| join_filter | postgres | 7644.521 | 4.83× | validated |
| join_filter | mysql | 26635.438 | 16.83× | validated |
| join_filter | mongo | 240647.750 | 152.03× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 7841.646 | 4.95× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
