# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.992 | 8.92× | validated |
| point_get | duckdb | 41.720 | 374.96× | validated |
| point_get | postgres | 264.476 | 2377.04× | validated |
| point_get | mysql | 345.332 | 3103.76× | validated |
| point_get | mongo | 469.795 | 4222.41× | validated |
| point_get | pandas | 4.427 | 39.79× | validated |
| point_get | lin | 0.111 | 1.00× | validated |
| filter_eq | sqlite | 700.372 | 808.59× | validated |
| filter_eq | duckdb | 434.867 | 502.06× | validated |
| filter_eq | postgres | 2492.375 | 2877.47× | validated |
| filter_eq | mysql | 3564.854 | 4115.66× | validated |
| filter_eq | mongo | 5351.208 | 6178.03× | validated |
| filter_eq | pandas | 2271.000 | 2621.89× | validated |
| filter_eq | lin | 0.866 | 1.00× | validated |
| text_substr | sqlite | 3593.541 | 17.41× | validated |
| text_substr | duckdb | 486.308 | 2.36× | validated |
| text_substr | postgres | 5667.437 | 27.46× | validated |
| text_substr | mysql | 9929.646 | 48.10× | validated |
| text_substr | mongo | 28361.896 | 137.40× | validated |
| text_substr | pandas | 4928.792 | 23.88× | validated |
| text_substr | lin | 206.425 | 1.00× | validated |
| materialize | sqlite | 21005.334 | 3.22× | validated |
| materialize | duckdb | 12852.917 | 1.97× | validated |
| materialize | postgres | 18481.084 | 2.83× | validated |
| materialize | mysql | 83859.979 | 12.86× | validated |
| materialize | mongo | 44275.459 | 6.79× | validated |
| materialize | pandas | 19111.188 | 2.93× | validated |
| materialize | lin | 6519.145 | 1.00× | validated |
| join_inner | sqlite | 53205.355 | 3.19× | validated |
| join_inner | duckdb | 29369.209 | 1.76× | validated |
| join_inner | postgres | 48775.376 | 2.93× | validated |
| join_inner | mysql | 172402.729 | 10.35× | validated |
| join_inner | mongo | 1169392.188 | 70.19× | validated |
| join_inner | pandas | 47371.771 | 2.84× | validated |
| join_inner | lin | 16661.188 | 1.00× | validated |
| join_filter | sqlite | 26318.562 | 3.17× | validated |
| join_filter | duckdb | 14525.875 | 1.75× | validated |
| join_filter | postgres | 22672.417 | 2.73× | validated |
| join_filter | mysql | 91279.083 | 11.00× | validated |
| join_filter | mongo | 615432.229 | 74.17× | validated |
| join_filter | pandas | 19867.834 | 2.39× | validated |
| join_filter | lin | 8297.291 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
