# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.370 | 1.00× | validated |
| point_get | sqlite | 1.326 | 3.58× | validated |
| point_get | duckdb | 46.442 | 125.39× | validated |
| point_get | postgres | 261.900 | 707.09× | validated |
| point_get | mysql | 365.359 | 986.42× | validated |
| point_get | mongo | 490.188 | 1323.44× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 10.155 | 27.42× | validated |
| filter_eq | lin | 0.960 | 1.00× | validated |
| filter_eq | sqlite | 96.780 | 100.81× | validated |
| filter_eq | duckdb | 249.854 | 260.26× | validated |
| filter_eq | postgres | 547.965 | 570.78× | validated |
| filter_eq | mysql | 785.024 | 817.71× | validated |
| filter_eq | mongo | 1000.734 | 1042.40× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 623.838 | 649.81× | validated |
| text_substr | lin | 34.229 | 1.00× | validated |
| text_substr | sqlite | 446.540 | 13.05× | validated |
| text_substr | duckdb | 130.537 | 3.81× | validated |
| text_substr | postgres | 919.196 | 26.85× | validated |
| text_substr | mysql | 1645.195 | 48.06× | validated |
| text_substr | mongo | 3932.854 | 114.90× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 1130.964 | 33.04× | validated |
| materialize | lin | 558.659 | 1.00× | validated |
| materialize | sqlite | 2564.771 | 4.59× | validated |
| materialize | duckdb | 1779.438 | 3.19× | validated |
| materialize | postgres | 2467.208 | 4.42× | validated |
| materialize | mysql | 13710.104 | 24.54× | validated |
| materialize | mongo | 7559.333 | 13.53× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 4750.855 | 8.50× | validated |
| join_inner | lin | 1820.938 | 1.00× | validated |
| join_inner | sqlite | 6365.958 | 3.50× | validated |
| join_inner | duckdb | 3689.729 | 2.03× | validated |
| join_inner | postgres | 6090.938 | 3.34× | validated |
| join_inner | mysql | 29206.625 | 16.04× | validated |
| join_inner | mongo | 174507.084 | 95.83× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 29519.250 | 16.21× | validated |
| join_filter | lin | 898.375 | 1.00× | validated |
| join_filter | sqlite | 3278.604 | 3.65× | validated |
| join_filter | duckdb | 1993.636 | 2.22× | validated |
| join_filter | postgres | 3300.458 | 3.67× | validated |
| join_filter | mysql | 16405.750 | 18.26× | validated |
| join_filter | mongo | 304562.104 | 339.01× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 9028.396 | 10.05× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
