# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.391 | 1.00× | validated |
| point_get | sqlite | 1.332 | 3.40× | validated |
| point_get | duckdb | 45.556 | 116.43× | validated |
| point_get | postgres | 302.243 | 772.47× | validated |
| point_get | mysql | 361.570 | 924.09× | validated |
| point_get | mongo | 486.807 | 1244.17× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.517 | 16.66× | validated |
| filter_eq | lin | 0.969 | 1.00× | validated |
| filter_eq | sqlite | 10.550 | 10.88× | validated |
| filter_eq | duckdb | 172.333 | 177.78× | validated |
| filter_eq | postgres | 352.198 | 363.34× | validated |
| filter_eq | mysql | 399.786 | 412.43× | validated |
| filter_eq | mongo | 563.192 | 581.01× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 80.300 | 82.84× | validated |
| text_substr | lin | 3.006 | 1.00× | validated |
| text_substr | sqlite | 44.924 | 14.94× | validated |
| text_substr | duckdb | 80.451 | 26.76× | validated |
| text_substr | postgres | 385.495 | 128.22× | validated |
| text_substr | mysql | 464.345 | 154.45× | validated |
| text_substr | mongo | 878.312 | 292.14× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 108.748 | 36.17× | validated |
| materialize | lin | 57.797 | 1.00× | validated |
| materialize | sqlite | 234.039 | 4.05× | validated |
| materialize | duckdb | 417.441 | 7.22× | validated |
| materialize | postgres | 505.781 | 8.75× | validated |
| materialize | mysql | 1586.250 | 27.44× | validated |
| materialize | mongo | 1518.472 | 26.27× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 546.930 | 9.46× | validated |
| join_inner | lin | 177.385 | 1.00× | validated |
| join_inner | sqlite | 547.576 | 3.09× | validated |
| join_inner | duckdb | 502.206 | 2.83× | validated |
| join_inner | postgres | 789.892 | 4.45× | validated |
| join_inner | mysql | 2983.521 | 16.82× | validated |
| join_inner | mongo | 18301.292 | 103.17× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 992.458 | 5.59× | validated |
| join_filter | lin | 96.071 | 1.00× | validated |
| join_filter | sqlite | 281.086 | 2.93× | validated |
| join_filter | duckdb | 330.298 | 3.44× | validated |
| join_filter | postgres | 609.573 | 6.35× | validated |
| join_filter | mysql | 1755.156 | 18.27× | validated |
| join_filter | mongo | 9716.979 | 101.14× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 909.629 | 9.47× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
