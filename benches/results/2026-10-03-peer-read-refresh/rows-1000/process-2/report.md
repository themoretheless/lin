# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.958 | 3.77× | validated |
| point_get | duckdb | 41.518 | 163.61× | validated |
| point_get | postgres | 307.967 | 1213.58× | validated |
| point_get | mysql | 375.802 | 1480.90× | validated |
| point_get | mongo | 472.338 | 1861.31× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.546 | 17.92× | validated |
| point_get | lin | 0.254 | 1.00× | validated |
| filter_eq | sqlite | 7.711 | 10.98× | validated |
| filter_eq | duckdb | 142.929 | 203.61× | validated |
| filter_eq | postgres | 348.540 | 496.52× | validated |
| filter_eq | mysql | 387.069 | 551.41× | validated |
| filter_eq | mongo | 563.859 | 803.25× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 59.399 | 84.62× | validated |
| filter_eq | lin | 0.702 | 1.00× | validated |
| text_substr | sqlite | 33.536 | 16.59× | validated |
| text_substr | duckdb | 65.580 | 32.44× | validated |
| text_substr | postgres | 358.109 | 177.15× | validated |
| text_substr | mysql | 434.568 | 214.98× | validated |
| text_substr | mongo | 836.090 | 413.60× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 78.651 | 38.91× | validated |
| text_substr | lin | 2.021 | 1.00× | validated |
| materialize | sqlite | 172.344 | 4.11× | validated |
| materialize | duckdb | 294.201 | 7.02× | validated |
| materialize | postgres | 445.845 | 10.64× | validated |
| materialize | mysql | 1347.805 | 32.17× | validated |
| materialize | mongo | 1332.924 | 31.82× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 385.125 | 9.19× | validated |
| materialize | lin | 41.892 | 1.00× | validated |
| join_inner | sqlite | 392.904 | 2.93× | validated |
| join_inner | duckdb | 363.107 | 2.71× | validated |
| join_inner | postgres | 680.757 | 5.08× | validated |
| join_inner | mysql | 2382.167 | 17.78× | validated |
| join_inner | mongo | 13618.541 | 101.65× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 751.458 | 5.61× | validated |
| join_inner | lin | 133.974 | 1.00× | validated |
| join_filter | sqlite | 203.528 | 3.02× | validated |
| join_filter | duckdb | 263.043 | 3.90× | validated |
| join_filter | postgres | 541.031 | 8.02× | validated |
| join_filter | mysql | 1368.854 | 20.28× | validated |
| join_filter | mongo | 8346.229 | 123.66× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 667.827 | 9.89× | validated |
| join_filter | lin | 67.496 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
