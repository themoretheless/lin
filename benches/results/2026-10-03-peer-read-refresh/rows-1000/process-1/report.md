# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.245 | 1.00× | validated |
| point_get | sqlite | 0.961 | 3.93× | validated |
| point_get | duckdb | 39.056 | 159.57× | validated |
| point_get | postgres | 299.033 | 1221.73× | validated |
| point_get | mysql | 366.127 | 1495.85× | validated |
| point_get | mongo | 468.719 | 1914.99× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.557 | 18.62× | validated |
| filter_eq | lin | 0.676 | 1.00× | validated |
| filter_eq | sqlite | 7.822 | 11.57× | validated |
| filter_eq | duckdb | 142.833 | 211.29× | validated |
| filter_eq | postgres | 329.354 | 487.22× | validated |
| filter_eq | mysql | 389.894 | 576.78× | validated |
| filter_eq | mongo | 540.387 | 799.40× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 64.659 | 95.65× | validated |
| text_substr | lin | 1.877 | 1.00× | validated |
| text_substr | sqlite | 32.344 | 17.23× | validated |
| text_substr | duckdb | 63.543 | 33.85× | validated |
| text_substr | postgres | 353.203 | 188.17× | validated |
| text_substr | mysql | 447.485 | 238.40× | validated |
| text_substr | mongo | 797.733 | 424.99× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 84.515 | 45.03× | validated |
| materialize | lin | 42.726 | 1.00× | validated |
| materialize | sqlite | 174.202 | 4.08× | validated |
| materialize | duckdb | 295.641 | 6.92× | validated |
| materialize | postgres | 431.826 | 10.11× | validated |
| materialize | mysql | 1330.993 | 31.15× | validated |
| materialize | mongo | 1396.722 | 32.69× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 374.163 | 8.76× | validated |
| join_inner | lin | 137.190 | 1.00× | validated |
| join_inner | sqlite | 401.406 | 2.93× | validated |
| join_inner | duckdb | 365.998 | 2.67× | validated |
| join_inner | postgres | 697.316 | 5.08× | validated |
| join_inner | mysql | 2377.323 | 17.33× | validated |
| join_inner | mongo | 12858.375 | 93.73× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 720.476 | 5.25× | validated |
| join_filter | lin | 65.744 | 1.00× | validated |
| join_filter | sqlite | 208.673 | 3.17× | validated |
| join_filter | duckdb | 250.982 | 3.82× | validated |
| join_filter | postgres | 499.690 | 7.60× | validated |
| join_filter | mysql | 1446.854 | 22.01× | validated |
| join_filter | mongo | 8120.167 | 123.51× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 628.917 | 9.57× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
