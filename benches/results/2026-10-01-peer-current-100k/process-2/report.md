# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 1.038 | 2.79× | validated |
| point_get | duckdb | 74.307 | 199.48× | validated |
| point_get | postgres | 245.314 | 658.55× | validated |
| point_get | mysql | 943.052 | 2531.65× | validated |
| point_get | mongo | 915.833 | 2458.59× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.755 | 18.13× | validated |
| point_get | lin | 0.373 | 1.00× | validated |
| filter_eq | sqlite | 699.030 | 419.64× | validated |
| filter_eq | duckdb | 587.156 | 352.48× | validated |
| filter_eq | postgres | 3341.667 | 2006.07× | validated |
| filter_eq | mysql | 5189.770 | 3115.53× | validated |
| filter_eq | mongo | 5874.791 | 3526.76× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 4338.625 | 2604.57× | validated |
| filter_eq | lin | 1.666 | 1.00× | validated |
| text_substr | sqlite | 3578.584 | 9.84× | validated |
| text_substr | duckdb | 1525.396 | 4.20× | validated |
| text_substr | postgres | 6405.041 | 17.62× | validated |
| text_substr | mysql | 13330.000 | 36.67× | validated |
| text_substr | mongo | 30695.125 | 84.43× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 11484.458 | 31.59× | validated |
| text_substr | lin | 363.554 | 1.00× | validated |
| materialize | sqlite | 21960.792 | 1.31× | validated |
| materialize | duckdb | 49900.625 | 2.99× | validated |
| materialize | postgres | 19632.167 | 1.17× | validated |
| materialize | mysql | 126514.125 | 7.57× | validated |
| materialize | mongo | 66817.958 | 4.00× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 38948.875 | 2.33× | validated |
| materialize | lin | 16710.750 | 1.00× | validated |
| join_inner | sqlite | 84809.875 | 2.29× | validated |
| join_inner | duckdb | 64739.459 | 1.75× | validated |
| join_inner | postgres | 46244.709 | 1.25× | validated |
| join_inner | mysql | 477177.792 | 12.88× | validated |
| join_inner | mongo | 1907699.334 | 51.48× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 197123.583 | 5.32× | validated |
| join_inner | lin | 37059.834 | 1.00× | validated |
| join_filter | sqlite | 64042.250 | 3.59× | validated |
| join_filter | duckdb | 20762.959 | 1.16× | validated |
| join_filter | postgres | 28539.500 | 1.60× | validated |
| join_filter | mysql | 265309.958 | 14.87× | validated |
| join_filter | mongo | 1026089.917 | 57.50× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 125420.584 | 7.03× | validated |
| join_filter | lin | 17844.292 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
