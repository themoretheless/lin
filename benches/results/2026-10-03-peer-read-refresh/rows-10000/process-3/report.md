# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 41.322 | 167.23× | validated |
| point_get | postgres | 271.838 | 1100.13× | validated |
| point_get | mysql | 290.321 | 1174.93× | validated |
| point_get | mongo | 448.197 | 1813.86× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.417 | 17.87× | validated |
| point_get | lin | 0.247 | 1.00× | validated |
| point_get | sqlite | 0.965 | 3.90× | validated |
| filter_eq | duckdb | 180.150 | 263.32× | validated |
| filter_eq | postgres | 487.971 | 713.25× | validated |
| filter_eq | mysql | 606.417 | 886.38× | validated |
| filter_eq | mongo | 806.808 | 1179.28× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 262.681 | 383.95× | validated |
| filter_eq | lin | 0.684 | 1.00× | validated |
| filter_eq | sqlite | 69.966 | 102.27× | validated |
| text_substr | duckdb | 98.647 | 5.70× | validated |
| text_substr | postgres | 778.479 | 44.95× | validated |
| text_substr | mysql | 1263.385 | 72.95× | validated |
| text_substr | mongo | 2880.521 | 166.34× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 503.404 | 29.07× | validated |
| text_substr | lin | 17.318 | 1.00× | validated |
| text_substr | sqlite | 331.234 | 19.13× | validated |
| materialize | duckdb | 1235.495 | 3.02× | validated |
| materialize | postgres | 1683.458 | 4.12× | validated |
| materialize | mysql | 9672.250 | 23.67× | validated |
| materialize | mongo | 5239.896 | 12.82× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 1822.208 | 4.46× | validated |
| materialize | lin | 408.611 | 1.00× | validated |
| materialize | sqlite | 1983.177 | 4.85× | validated |
| join_inner | duckdb | 2679.604 | 1.97× | validated |
| join_inner | postgres | 3860.562 | 2.84× | validated |
| join_inner | mysql | 20731.375 | 15.27× | validated |
| join_inner | mongo | 113076.687 | 83.28× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 3952.438 | 2.91× | validated |
| join_inner | lin | 1357.840 | 1.00× | validated |
| join_inner | sqlite | 4529.249 | 3.34× | validated |
| join_filter | duckdb | 1423.611 | 2.18× | validated |
| join_filter | postgres | 2214.427 | 3.39× | validated |
| join_filter | mysql | 10877.062 | 16.67× | validated |
| join_filter | mongo | 56145.438 | 86.03× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 2421.271 | 3.71× | validated |
| join_filter | lin | 652.637 | 1.00× | validated |
| join_filter | sqlite | 2362.667 | 3.62× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
