# Validated read API benchmark

Rows: 1000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.393 | 1.00× | validated |
| point_get | sqlite | 1.311 | 3.33× | validated |
| point_get | duckdb | 46.135 | 117.35× | validated |
| point_get | postgres | 314.640 | 800.36× | validated |
| point_get | mysql | 361.570 | 919.73× | validated |
| point_get | mongo | 486.807 | 1238.30× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.372 | 16.21× | validated |
| filter_eq | lin | 0.969 | 1.00× | validated |
| filter_eq | sqlite | 10.678 | 11.02× | validated |
| filter_eq | duckdb | 173.151 | 178.63× | validated |
| filter_eq | postgres | 352.616 | 363.77× | validated |
| filter_eq | mysql | 403.693 | 416.46× | validated |
| filter_eq | mongo | 583.740 | 602.20× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 80.300 | 82.84× | validated |
| text_substr | lin | 3.006 | 1.00× | validated |
| text_substr | sqlite | 44.924 | 14.94× | validated |
| text_substr | duckdb | 80.451 | 26.76× | validated |
| text_substr | postgres | 385.495 | 128.22× | validated |
| text_substr | mysql | 489.260 | 162.74× | validated |
| text_substr | mongo | 904.604 | 300.89× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 108.748 | 36.17× | validated |
| materialize | lin | 58.199 | 1.00× | validated |
| materialize | sqlite | 236.935 | 4.07× | validated |
| materialize | duckdb | 401.016 | 6.89× | validated |
| materialize | postgres | 505.781 | 8.69× | validated |
| materialize | mysql | 1725.761 | 29.65× | validated |
| materialize | mongo | 1518.472 | 26.09× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 546.930 | 9.40× | validated |
| join_inner | lin | 177.385 | 1.00× | validated |
| join_inner | sqlite | 549.435 | 3.10× | validated |
| join_inner | duckdb | 502.206 | 2.83× | validated |
| join_inner | postgres | 793.167 | 4.47× | validated |
| join_inner | mysql | 3131.729 | 17.66× | validated |
| join_inner | mongo | 18301.292 | 103.17× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 1056.078 | 5.95× | validated |
| join_filter | lin | 96.071 | 1.00× | validated |
| join_filter | sqlite | 282.299 | 2.94× | validated |
| join_filter | duckdb | 330.324 | 3.44× | validated |
| join_filter | postgres | 609.573 | 6.35× | validated |
| join_filter | mysql | 1776.958 | 18.50× | validated |
| join_filter | mongo | 10445.624 | 108.73× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 909.629 | 9.47× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
