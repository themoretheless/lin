# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 1.357 | 3.69× | validated |
| point_get | duckdb | 48.574 | 132.15× | validated |
| point_get | postgres | 327.365 | 890.62× | validated |
| point_get | mysql | 423.639 | 1152.54× | validated |
| point_get | mongo | 554.390 | 1508.25× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.277 | 17.08× | validated |
| point_get | lin | 0.368 | 1.00× | validated |
| filter_eq | sqlite | 962.304 | 1004.82× | validated |
| filter_eq | duckdb | 584.706 | 610.54× | validated |
| filter_eq | postgres | 3246.292 | 3389.70× | validated |
| filter_eq | mysql | 4613.479 | 4817.29× | validated |
| filter_eq | mongo | 5412.083 | 5651.17× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 3151.688 | 3290.92× | validated |
| filter_eq | lin | 0.958 | 1.00× | validated |
| text_substr | sqlite | 4856.417 | 17.03× | validated |
| text_substr | duckdb | 633.938 | 2.22× | validated |
| text_substr | postgres | 7115.333 | 24.95× | validated |
| text_substr | mysql | 13164.292 | 46.16× | validated |
| text_substr | mongo | 32691.688 | 114.63× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 6369.270 | 22.33× | validated |
| text_substr | lin | 285.195 | 1.00× | validated |
| materialize | sqlite | 29434.209 | 4.30× | validated |
| materialize | duckdb | 16849.250 | 2.46× | validated |
| materialize | postgres | 23401.791 | 3.42× | validated |
| materialize | mysql | 118855.792 | 17.35× | validated |
| materialize | mongo | 59779.291 | 8.73× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 24208.084 | 3.53× | validated |
| materialize | lin | 6849.521 | 1.00× | validated |
| join_inner | sqlite | 72041.563 | 3.86× | validated |
| join_inner | duckdb | 40100.812 | 2.15× | validated |
| join_inner | postgres | 61243.896 | 3.28× | validated |
| join_inner | mysql | 236651.187 | 12.68× | validated |
| join_inner | mongo | 1584562.354 | 84.90× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 49905.979 | 2.67× | validated |
| join_inner | lin | 18663.791 | 1.00× | validated |
| join_filter | sqlite | 35326.438 | 3.83× | validated |
| join_filter | duckdb | 19921.729 | 2.16× | validated |
| join_filter | postgres | 32633.562 | 3.54× | validated |
| join_filter | mysql | 127174.042 | 13.79× | validated |
| join_filter | mongo | 826456.833 | 89.65× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 25972.791 | 2.82× | validated |
| join_filter | lin | 9218.959 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
