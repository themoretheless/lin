# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.250 | 1.00× | validated |
| point_get | sqlite | 1.038 | 4.15× | validated |
| point_get | duckdb | 49.434 | 197.62× | validated |
| point_get | postgres | 265.323 | 1060.66× | validated |
| point_get | mysql | 360.799 | 1442.33× | validated |
| point_get | mongo | 468.449 | 1872.67× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.950 | 19.79× | validated |
| filter_eq | lin | 0.797 | 1.00× | validated |
| filter_eq | sqlite | 699.030 | 877.19× | validated |
| filter_eq | duckdb | 479.454 | 601.65× | validated |
| filter_eq | postgres | 3341.667 | 4193.35× | validated |
| filter_eq | mysql | 3970.000 | 4981.82× | validated |
| filter_eq | mongo | 4235.521 | 5315.02× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 2446.000 | 3069.41× | validated |
| text_substr | lin | 207.823 | 1.00× | validated |
| text_substr | sqlite | 3550.750 | 17.09× | validated |
| text_substr | duckdb | 534.971 | 2.57× | validated |
| text_substr | postgres | 6405.041 | 30.82× | validated |
| text_substr | mysql | 10641.041 | 51.20× | validated |
| text_substr | mongo | 24116.417 | 116.04× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 5600.917 | 26.95× | validated |
| materialize | lin | 6028.625 | 1.00× | validated |
| materialize | sqlite | 21960.792 | 3.64× | validated |
| materialize | duckdb | 15795.958 | 2.62× | validated |
| materialize | postgres | 16232.959 | 2.69× | validated |
| materialize | mysql | 91822.292 | 15.23× | validated |
| materialize | mongo | 48193.125 | 7.99× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 19253.542 | 3.19× | validated |
| join_inner | lin | 14398.667 | 1.00× | validated |
| join_inner | sqlite | 52585.208 | 3.65× | validated |
| join_inner | duckdb | 29825.042 | 2.07× | validated |
| join_inner | postgres | 42348.583 | 2.94× | validated |
| join_inner | mysql | 178857.708 | 12.42× | validated |
| join_inner | mongo | 1163426.917 | 80.80× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 71675.250 | 4.98× | validated |
| join_filter | lin | 7265.625 | 1.00× | validated |
| join_filter | sqlite | 27394.541 | 3.77× | validated |
| join_filter | duckdb | 16029.416 | 2.21× | validated |
| join_filter | postgres | 22260.000 | 3.06× | validated |
| join_filter | mysql | 103737.958 | 14.28× | validated |
| join_filter | mongo | 615641.375 | 84.73× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 20554.458 | 2.83× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
