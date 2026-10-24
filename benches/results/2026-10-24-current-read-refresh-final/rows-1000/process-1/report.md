# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.113 | 1.00× | validated |
| point_get | sqlite | 0.902 | 8.00× | validated |
| point_get | duckdb | 38.900 | 344.87× | validated |
| point_get | postgres | 292.405 | 2592.34× | validated |
| point_get | mysql | 322.250 | 2856.93× | validated |
| point_get | mongo | 474.804 | 4209.41× | validated |
| point_get | pandas | 5.987 | 53.08× | validated |
| filter_eq | lin | 0.634 | 1.00× | validated |
| filter_eq | sqlite | 7.350 | 11.59× | validated |
| filter_eq | duckdb | 132.934 | 209.66× | validated |
| filter_eq | postgres | 321.668 | 507.33× | validated |
| filter_eq | mysql | 355.534 | 560.74× | validated |
| filter_eq | mongo | 480.370 | 757.63× | validated |
| filter_eq | pandas | 59.221 | 93.40× | validated |
| text_substr | lin | 1.520 | 1.00× | validated |
| text_substr | sqlite | 32.747 | 21.54× | validated |
| text_substr | duckdb | 57.194 | 37.63× | validated |
| text_substr | postgres | 333.256 | 219.23× | validated |
| text_substr | mysql | 405.333 | 266.65× | validated |
| text_substr | mongo | 681.176 | 448.11× | validated |
| text_substr | pandas | 72.311 | 47.57× | validated |
| materialize | lin | 40.123 | 1.00× | validated |
| materialize | sqlite | 159.112 | 3.97× | validated |
| materialize | duckdb | 292.579 | 7.29× | validated |
| materialize | postgres | 429.489 | 10.70× | validated |
| materialize | mysql | 1204.026 | 30.01× | validated |
| materialize | mongo | 1157.875 | 28.86× | validated |
| materialize | pandas | 370.948 | 9.25× | validated |
| join_inner | lin | 139.017 | 1.00× | validated |
| join_inner | sqlite | 373.601 | 2.69× | validated |
| join_inner | duckdb | 352.748 | 2.54× | validated |
| join_inner | postgres | 632.135 | 4.55× | validated |
| join_inner | mysql | 3079.959 | 22.16× | validated |
| join_inner | mongo | 14452.646 | 103.96× | validated |
| join_inner | pandas | 709.260 | 5.10× | validated |
| join_filter | lin | 67.622 | 1.00× | validated |
| join_filter | sqlite | 195.202 | 2.89× | validated |
| join_filter | duckdb | 246.999 | 3.65× | validated |
| join_filter | postgres | 485.035 | 7.17× | validated |
| join_filter | mysql | 1782.243 | 26.36× | validated |
| join_filter | mongo | 8611.291 | 127.35× | validated |
| join_filter | pandas | 618.119 | 9.14× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
