# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 130.009 | 175.54× | validated |
| point_get | postgres | 945.649 | 1276.81× | validated |
| point_get | mysql | 1207.786 | 1630.75× | validated |
| point_get | mongo | 953.879 | 1287.92× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 14.190 | 19.16× | validated |
| point_get | lin | 0.741 | 1.00× | validated |
| point_get | sqlite | 2.128 | 2.87× | validated |
| filter_eq | duckdb | 502.940 | 279.55× | validated |
| filter_eq | postgres | 1468.208 | 816.07× | validated |
| filter_eq | mysql | 1661.750 | 923.64× | validated |
| filter_eq | mongo | 1830.344 | 1017.35× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 689.196 | 383.07× | validated |
| filter_eq | lin | 1.799 | 1.00× | validated |
| filter_eq | sqlite | 218.172 | 121.27× | validated |
| text_substr | duckdb | 283.318 | 5.03× | validated |
| text_substr | postgres | 2102.041 | 37.29× | validated |
| text_substr | mysql | 2937.896 | 52.11× | validated |
| text_substr | mongo | 6574.896 | 116.63× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 1550.722 | 27.51× | validated |
| text_substr | lin | 56.374 | 1.00× | validated |
| text_substr | sqlite | 737.746 | 13.09× | validated |
| materialize | duckdb | 3101.104 | 1.97× | validated |
| materialize | postgres | 5167.187 | 3.28× | validated |
| materialize | mysql | 22031.604 | 13.99× | validated |
| materialize | mongo | 11795.396 | 7.49× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 5463.105 | 3.47× | validated |
| materialize | lin | 1574.296 | 1.00× | validated |
| materialize | sqlite | 5478.562 | 3.48× | validated |
| join_inner | duckdb | 7233.105 | 1.76× | validated |
| join_inner | postgres | 12249.812 | 2.97× | validated |
| join_inner | mysql | 47338.062 | 11.49× | validated |
| join_inner | mongo | 273561.230 | 66.42× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 10728.312 | 2.60× | validated |
| join_inner | lin | 4118.646 | 1.00× | validated |
| join_inner | sqlite | 13139.104 | 3.19× | validated |
| join_filter | duckdb | 3919.896 | 2.48× | validated |
| join_filter | postgres | 7644.521 | 4.83× | validated |
| join_filter | mysql | 26635.438 | 16.83× | validated |
| join_filter | mongo | 153498.541 | 96.97× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 7306.062 | 4.62× | validated |
| join_filter | lin | 1582.875 | 1.00× | validated |
| join_filter | sqlite | 6576.562 | 4.15× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
