# Validated read API benchmark

Rows: 1000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.113 | 1.00× | validated |
| point_get | sqlite | 0.902 | 7.96× | validated |
| point_get | duckdb | 38.900 | 343.24× | validated |
| point_get | postgres | 292.405 | 2580.04× | validated |
| point_get | mysql | 322.250 | 2843.38× | validated |
| point_get | mongo | 474.804 | 4189.44× | validated |
| point_get | pandas | 5.987 | 52.83× | validated |
| filter_eq | lin | 0.634 | 1.00× | validated |
| filter_eq | sqlite | 7.425 | 11.71× | validated |
| filter_eq | duckdb | 132.538 | 209.04× | validated |
| filter_eq | postgres | 321.668 | 507.33× | validated |
| filter_eq | mysql | 355.534 | 560.74× | validated |
| filter_eq | mongo | 480.370 | 757.63× | validated |
| filter_eq | pandas | 59.387 | 93.66× | validated |
| text_substr | lin | 1.640 | 1.00× | validated |
| text_substr | sqlite | 32.361 | 19.73× | validated |
| text_substr | duckdb | 57.194 | 34.87× | validated |
| text_substr | postgres | 333.256 | 203.16× | validated |
| text_substr | mysql | 405.333 | 247.10× | validated |
| text_substr | mongo | 681.176 | 415.26× | validated |
| text_substr | pandas | 73.930 | 45.07× | validated |
| materialize | lin | 39.411 | 1.00× | validated |
| materialize | sqlite | 162.030 | 4.11× | validated |
| materialize | duckdb | 292.579 | 7.42× | validated |
| materialize | postgres | 416.659 | 10.57× | validated |
| materialize | mysql | 1204.026 | 30.55× | validated |
| materialize | mongo | 1157.875 | 29.38× | validated |
| materialize | pandas | 370.948 | 9.41× | validated |
| join_inner | lin | 130.918 | 1.00× | validated |
| join_inner | sqlite | 383.369 | 2.93× | validated |
| join_inner | duckdb | 352.748 | 2.69× | validated |
| join_inner | postgres | 632.135 | 4.83× | validated |
| join_inner | mysql | 3079.959 | 23.53× | validated |
| join_inner | mongo | 13788.104 | 105.32× | validated |
| join_inner | pandas | 709.260 | 5.42× | validated |
| join_filter | lin | 67.320 | 1.00× | validated |
| join_filter | sqlite | 198.795 | 2.95× | validated |
| join_filter | duckdb | 246.071 | 3.66× | validated |
| join_filter | postgres | 485.035 | 7.20× | validated |
| join_filter | mysql | 1707.823 | 25.37× | validated |
| join_filter | mongo | 8354.646 | 124.10× | validated |
| join_filter | pandas | 623.008 | 9.25× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
