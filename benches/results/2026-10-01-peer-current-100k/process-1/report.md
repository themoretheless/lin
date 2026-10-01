# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.250 | 1.00× | validated |
| point_get | sqlite | 1.033 | 4.13× | validated |
| point_get | duckdb | 49.434 | 197.62× | validated |
| point_get | postgres | 265.323 | 1060.66× | validated |
| point_get | mysql | 360.799 | 1442.33× | validated |
| point_get | mongo | 467.123 | 1867.37× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.950 | 19.79× | validated |
| filter_eq | lin | 0.797 | 1.00× | validated |
| filter_eq | sqlite | 704.006 | 883.43× | validated |
| filter_eq | duckdb | 439.099 | 551.01× | validated |
| filter_eq | postgres | 3432.681 | 4307.56× | validated |
| filter_eq | mysql | 3592.916 | 4508.63× | validated |
| filter_eq | mongo | 4235.521 | 5315.02× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 2446.000 | 3069.41× | validated |
| text_substr | lin | 207.823 | 1.00× | validated |
| text_substr | sqlite | 3550.750 | 17.09× | validated |
| text_substr | duckdb | 534.971 | 2.57× | validated |
| text_substr | postgres | 6485.125 | 31.20× | validated |
| text_substr | mysql | 10002.584 | 48.13× | validated |
| text_substr | mongo | 23774.959 | 114.40× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 5600.917 | 26.95× | validated |
| materialize | lin | 5695.917 | 1.00× | validated |
| materialize | sqlite | 21243.917 | 3.73× | validated |
| materialize | duckdb | 15795.958 | 2.77× | validated |
| materialize | postgres | 14987.000 | 2.63× | validated |
| materialize | mysql | 86868.125 | 15.25× | validated |
| materialize | mongo | 45423.708 | 7.97× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 19253.542 | 3.38× | validated |
| join_inner | lin | 14248.959 | 1.00× | validated |
| join_inner | sqlite | 52149.416 | 3.66× | validated |
| join_inner | duckdb | 29825.042 | 2.09× | validated |
| join_inner | postgres | 40611.833 | 2.85× | validated |
| join_inner | mysql | 175780.458 | 12.34× | validated |
| join_inner | mongo | 1154165.417 | 81.00× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 71675.250 | 5.03× | validated |
| join_filter | lin | 7265.625 | 1.00× | validated |
| join_filter | sqlite | 26300.167 | 3.62× | validated |
| join_filter | duckdb | 16029.416 | 2.21× | validated |
| join_filter | postgres | 22260.000 | 3.06× | validated |
| join_filter | mysql | 93758.666 | 12.90× | validated |
| join_filter | mongo | 585696.583 | 80.61× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 20554.458 | 2.83× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
