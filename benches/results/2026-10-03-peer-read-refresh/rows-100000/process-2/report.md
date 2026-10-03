# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.982 | 2.21× | validated |
| point_get | duckdb | 40.013 | 90.13× | validated |
| point_get | postgres | 271.647 | 611.85× | validated |
| point_get | mysql | 366.962 | 826.54× | validated |
| point_get | mongo | 444.757 | 1001.77× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.710 | 10.61× | validated |
| point_get | lin | 0.444 | 1.00× | validated |
| filter_eq | sqlite | 705.976 | 827.39× | validated |
| filter_eq | duckdb | 416.713 | 488.38× | validated |
| filter_eq | postgres | 2329.958 | 2730.66× | validated |
| filter_eq | mysql | 3440.646 | 4032.35× | validated |
| filter_eq | mongo | 3792.521 | 4444.74× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 2318.792 | 2717.57× | validated |
| filter_eq | lin | 0.853 | 1.00× | validated |
| text_substr | sqlite | 3364.834 | 15.07× | validated |
| text_substr | duckdb | 465.071 | 2.08× | validated |
| text_substr | postgres | 5248.146 | 23.51× | validated |
| text_substr | mysql | 9826.750 | 44.01× | validated |
| text_substr | mongo | 24165.209 | 108.24× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 5165.104 | 23.13× | validated |
| text_substr | lin | 223.264 | 1.00× | validated |
| materialize | sqlite | 20899.688 | 3.76× | validated |
| materialize | duckdb | 12406.896 | 2.23× | validated |
| materialize | postgres | 14948.791 | 2.69× | validated |
| materialize | mysql | 85180.730 | 15.33× | validated |
| materialize | mongo | 45214.854 | 8.14× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 17799.480 | 3.20× | validated |
| materialize | lin | 5556.021 | 1.00× | validated |
| join_inner | sqlite | 52371.542 | 1.93× | validated |
| join_inner | duckdb | 28781.354 | 1.06× | validated |
| join_inner | postgres | 39920.084 | 1.47× | validated |
| join_inner | mysql | 174297.729 | 6.42× | validated |
| join_inner | mongo | 1113695.729 | 41.03× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 38287.854 | 1.41× | validated |
| join_inner | lin | 27143.729 | 1.00× | validated |
| join_filter | sqlite | 26031.896 | 3.43× | validated |
| join_filter | duckdb | 14055.542 | 1.85× | validated |
| join_filter | postgres | 21602.105 | 2.85× | validated |
| join_filter | mysql | 94459.042 | 12.45× | validated |
| join_filter | mongo | 571808.812 | 75.36× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 19478.375 | 2.57× | validated |
| join_filter | lin | 7587.500 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
