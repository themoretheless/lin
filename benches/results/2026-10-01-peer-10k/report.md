# Validated read API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.246 | 1.00× | validated |
| point_get | sqlite | 1.018 | 4.15× | validated |
| point_get | duckdb | 61.767 | 251.52× | validated |
| point_get | postgres | 550.058 | 2239.90× | validated |
| point_get | mysql | 331.253 | 1348.90× | validated |
| point_get | mongo | 576.513 | 2347.62× | validated |
| point_get | pandas | 4.707 | 19.17× | validated |
| filter_eq | lin | 0.775 | 1.00× | validated |
| filter_eq | sqlite | 78.198 | 100.95× | validated |
| filter_eq | duckdb | 271.081 | 349.95× | validated |
| filter_eq | postgres | 873.839 | 1128.08× | validated |
| filter_eq | mysql | 773.569 | 998.64× | validated |
| filter_eq | mongo | 1374.062 | 1773.84× | validated |
| filter_eq | pandas | 273.132 | 352.60× | validated |
| text_substr | lin | 83.547 | 1.00× | validated |
| text_substr | sqlite | 374.995 | 4.49× | validated |
| text_substr | duckdb | 117.022 | 1.40× | validated |
| text_substr | postgres | 1385.438 | 16.58× | validated |
| text_substr | mysql | 2228.958 | 26.68× | validated |
| text_substr | mongo | 4065.396 | 48.66× | validated |
| text_substr | pandas | 509.968 | 6.10× | validated |
| materialize | lin | 425.625 | 1.00× | validated |
| materialize | sqlite | 2187.854 | 5.14× | validated |
| materialize | duckdb | 1778.052 | 4.18× | validated |
| materialize | postgres | 2863.146 | 6.73× | validated |
| materialize | mysql | 13446.458 | 31.59× | validated |
| materialize | mongo | 6877.979 | 16.16× | validated |
| materialize | pandas | 1925.750 | 4.52× | validated |
| join_inner | lin | 1379.090 | 1.00× | validated |
| join_inner | sqlite | 5190.730 | 3.76× | validated |
| join_inner | duckdb | 3761.416 | 2.73× | validated |
| join_inner | postgres | 5504.917 | 3.99× | validated |
| join_inner | mysql | 26859.271 | 19.48× | validated |
| join_inner | mongo | 145461.062 | 105.48× | validated |
| join_inner | pandas | 4177.229 | 3.03× | validated |
| join_filter | lin | 676.390 | 1.00× | validated |
| join_filter | sqlite | 2649.834 | 3.92× | validated |
| join_filter | duckdb | 2153.552 | 3.18× | validated |
| join_filter | postgres | 3253.062 | 4.81× | validated |
| join_filter | mysql | 13119.646 | 19.40× | validated |
| join_filter | mongo | 59697.854 | 88.26× | validated |
| join_filter | pandas | 2484.167 | 3.67× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
