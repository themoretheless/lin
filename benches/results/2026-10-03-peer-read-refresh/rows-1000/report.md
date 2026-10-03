# Validated read API benchmark

Rows: 1000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.251 | 1.00× | validated |
| point_get | sqlite | 0.958 | 3.82× | validated |
| point_get | duckdb | 40.026 | 159.53× | validated |
| point_get | postgres | 307.967 | 1227.45× | validated |
| point_get | mysql | 366.127 | 1459.25× | validated |
| point_get | mongo | 472.338 | 1882.57× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.557 | 18.16× | validated |
| filter_eq | lin | 0.694 | 1.00× | validated |
| filter_eq | sqlite | 7.711 | 11.10× | validated |
| filter_eq | duckdb | 142.833 | 205.67× | validated |
| filter_eq | postgres | 332.556 | 478.86× | validated |
| filter_eq | mysql | 387.069 | 557.36× | validated |
| filter_eq | mongo | 563.859 | 811.93× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 59.399 | 85.53× | validated |
| text_substr | lin | 1.877 | 1.00× | validated |
| text_substr | sqlite | 33.005 | 17.58× | validated |
| text_substr | duckdb | 64.037 | 34.12× | validated |
| text_substr | postgres | 357.811 | 190.62× | validated |
| text_substr | mysql | 447.485 | 238.40× | validated |
| text_substr | mongo | 836.090 | 445.43× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 78.651 | 41.90× | validated |
| materialize | lin | 41.892 | 1.00× | validated |
| materialize | sqlite | 174.202 | 4.16× | validated |
| materialize | duckdb | 295.641 | 7.06× | validated |
| materialize | postgres | 442.286 | 10.56× | validated |
| materialize | mysql | 1347.805 | 32.17× | validated |
| materialize | mongo | 1396.722 | 33.34× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 385.125 | 9.19× | validated |
| join_inner | lin | 137.190 | 1.00× | validated |
| join_inner | sqlite | 401.406 | 2.93× | validated |
| join_inner | duckdb | 364.548 | 2.66× | validated |
| join_inner | postgres | 695.211 | 5.07× | validated |
| join_inner | mysql | 2382.167 | 17.36× | validated |
| join_inner | mongo | 13618.541 | 99.27× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 751.458 | 5.48× | validated |
| join_filter | lin | 67.496 | 1.00× | validated |
| join_filter | sqlite | 208.673 | 3.09× | validated |
| join_filter | duckdb | 252.616 | 3.74× | validated |
| join_filter | postgres | 516.356 | 7.65× | validated |
| join_filter | mysql | 1446.854 | 21.44× | validated |
| join_filter | mongo | 8346.229 | 123.66× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 667.827 | 9.89× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
