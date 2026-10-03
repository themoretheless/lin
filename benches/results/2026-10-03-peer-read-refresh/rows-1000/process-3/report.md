# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 40.026 | 159.53× | validated |
| point_get | postgres | 312.443 | 1245.29× | validated |
| point_get | mysql | 352.933 | 1406.67× | validated |
| point_get | mongo | 478.748 | 1908.12× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.829 | 19.25× | validated |
| point_get | lin | 0.251 | 1.00× | validated |
| point_get | sqlite | 0.938 | 3.74× | validated |
| filter_eq | duckdb | 138.738 | 199.78× | validated |
| filter_eq | postgres | 332.556 | 478.86× | validated |
| filter_eq | mysql | 365.732 | 526.64× | validated |
| filter_eq | mongo | 616.321 | 887.47× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 58.545 | 84.30× | validated |
| filter_eq | lin | 0.694 | 1.00× | validated |
| filter_eq | sqlite | 7.663 | 11.03× | validated |
| text_substr | duckdb | 64.037 | 34.37× | validated |
| text_substr | postgres | 357.811 | 192.06× | validated |
| text_substr | mysql | 458.155 | 245.92× | validated |
| text_substr | mongo | 836.562 | 449.04× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 78.030 | 41.88× | validated |
| text_substr | lin | 1.863 | 1.00× | validated |
| text_substr | sqlite | 33.005 | 17.72× | validated |
| materialize | duckdb | 297.844 | 7.21× | validated |
| materialize | postgres | 442.286 | 10.70× | validated |
| materialize | mysql | 1393.569 | 33.73× | validated |
| materialize | mongo | 1475.396 | 35.71× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 397.769 | 9.63× | validated |
| materialize | lin | 41.319 | 1.00× | validated |
| materialize | sqlite | 174.858 | 4.23× | validated |
| join_inner | duckdb | 364.548 | 2.64× | validated |
| join_inner | postgres | 695.211 | 5.04× | validated |
| join_inner | mysql | 2586.490 | 18.74× | validated |
| join_inner | mongo | 13833.563 | 100.25× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 758.201 | 5.49× | validated |
| join_inner | lin | 137.990 | 1.00× | validated |
| join_inner | sqlite | 413.634 | 3.00× | validated |
| join_filter | duckdb | 252.616 | 3.67× | validated |
| join_filter | postgres | 516.356 | 7.50× | validated |
| join_filter | mysql | 1552.917 | 22.55× | validated |
| join_filter | mongo | 8679.625 | 126.05× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 673.295 | 9.78× | validated |
| join_filter | lin | 68.858 | 1.00× | validated |
| join_filter | sqlite | 209.153 | 3.04× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
