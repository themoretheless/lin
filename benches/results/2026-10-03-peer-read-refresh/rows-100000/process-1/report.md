# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.244 | 1.00× | validated |
| point_get | sqlite | 1.013 | 4.14× | validated |
| point_get | duckdb | 39.973 | 163.56× | validated |
| point_get | postgres | 280.072 | 1146.00× | validated |
| point_get | mysql | 349.969 | 1432.01× | validated |
| point_get | mongo | 432.362 | 1769.15× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.401 | 18.01× | validated |
| filter_eq | lin | 0.738 | 1.00× | validated |
| filter_eq | sqlite | 700.128 | 948.77× | validated |
| filter_eq | duckdb | 438.163 | 593.77× | validated |
| filter_eq | postgres | 2611.250 | 3538.61× | validated |
| filter_eq | mysql | 3354.959 | 4546.44× | validated |
| filter_eq | mongo | 3913.541 | 5303.40× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 2329.406 | 3156.67× | validated |
| text_substr | lin | 208.178 | 1.00× | validated |
| text_substr | sqlite | 3327.271 | 15.98× | validated |
| text_substr | duckdb | 470.685 | 2.26× | validated |
| text_substr | postgres | 5268.812 | 25.31× | validated |
| text_substr | mysql | 9586.312 | 46.05× | validated |
| text_substr | mongo | 23570.187 | 113.22× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 4469.500 | 21.47× | validated |
| materialize | lin | 5363.209 | 1.00× | validated |
| materialize | sqlite | 20274.854 | 3.78× | validated |
| materialize | duckdb | 12346.834 | 2.30× | validated |
| materialize | postgres | 15143.000 | 2.82× | validated |
| materialize | mysql | 87109.583 | 16.24× | validated |
| materialize | mongo | 44764.583 | 8.35× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 17064.625 | 3.18× | validated |
| join_inner | lin | 14019.188 | 1.00× | validated |
| join_inner | sqlite | 51422.916 | 3.67× | validated |
| join_inner | duckdb | 29679.395 | 2.12× | validated |
| join_inner | postgres | 40420.854 | 2.88× | validated |
| join_inner | mysql | 173184.416 | 12.35× | validated |
| join_inner | mongo | 1124911.250 | 80.24× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 36313.875 | 2.59× | validated |
| join_filter | lin | 7046.855 | 1.00× | validated |
| join_filter | sqlite | 25415.312 | 3.61× | validated |
| join_filter | duckdb | 14270.688 | 2.03× | validated |
| join_filter | postgres | 21915.646 | 3.11× | validated |
| join_filter | mysql | 93154.583 | 13.22× | validated |
| join_filter | mongo | 561053.250 | 79.62× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 18780.854 | 2.67× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
