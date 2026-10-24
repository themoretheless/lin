# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 38.004 | 331.36× | validated |
| point_get | postgres | 323.759 | 2822.91× | validated |
| point_get | mysql | 330.543 | 2882.07× | validated |
| point_get | mongo | 470.891 | 4105.79× | validated |
| point_get | pandas | 4.439 | 38.71× | validated |
| point_get | lin | 0.115 | 1.00× | validated |
| point_get | sqlite | 0.941 | 8.21× | validated |
| filter_eq | duckdb | 139.964 | 187.70× | validated |
| filter_eq | postgres | 341.042 | 457.35× | validated |
| filter_eq | mysql | 358.695 | 481.02× | validated |
| filter_eq | mongo | 542.185 | 727.09× | validated |
| filter_eq | pandas | 56.342 | 75.56× | validated |
| filter_eq | lin | 0.746 | 1.00× | validated |
| filter_eq | sqlite | 7.565 | 10.14× | validated |
| text_substr | duckdb | 66.988 | 28.22× | validated |
| text_substr | postgres | 363.364 | 153.05× | validated |
| text_substr | mysql | 401.203 | 168.99× | validated |
| text_substr | mongo | 822.094 | 346.27× | validated |
| text_substr | pandas | 76.567 | 32.25× | validated |
| text_substr | lin | 2.374 | 1.00× | validated |
| text_substr | sqlite | 31.155 | 13.12× | validated |
| materialize | duckdb | 302.042 | 8.20× | validated |
| materialize | postgres | 448.269 | 12.17× | validated |
| materialize | mysql | 1208.764 | 32.81× | validated |
| materialize | mongo | 1388.826 | 37.70× | validated |
| materialize | pandas | 379.943 | 10.31× | validated |
| materialize | lin | 36.841 | 1.00× | validated |
| materialize | sqlite | 169.065 | 4.59× | validated |
| join_inner | duckdb | 408.569 | 3.12× | validated |
| join_inner | postgres | 659.485 | 5.04× | validated |
| join_inner | mysql | 2360.052 | 18.03× | validated |
| join_inner | mongo | 12630.250 | 96.50× | validated |
| join_inner | pandas | 720.281 | 5.50× | validated |
| join_inner | lin | 130.885 | 1.00× | validated |
| join_inner | sqlite | 386.325 | 2.95× | validated |
| join_filter | duckdb | 243.863 | 3.76× | validated |
| join_filter | postgres | 497.739 | 7.68× | validated |
| join_filter | mysql | 1569.438 | 24.22× | validated |
| join_filter | mongo | 6608.292 | 101.97× | validated |
| join_filter | pandas | 627.080 | 9.68× | validated |
| join_filter | lin | 64.807 | 1.00× | validated |
| join_filter | sqlite | 201.593 | 3.11× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
