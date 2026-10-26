# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.115 | 1.00× | validated |
| point_get | sqlite | 0.984 | 8.54× | validated |
| point_get | duckdb | 41.221 | 357.70× | validated |
| point_get | postgres | 272.472 | 2364.40× | validated |
| point_get | mysql | 347.831 | 3018.33× | validated |
| point_get | mongo | 484.028 | 4200.19× | validated |
| point_get | pandas | 4.473 | 38.81× | validated |
| filter_eq | lin | 0.709 | 1.00× | validated |
| filter_eq | sqlite | 69.799 | 98.40× | validated |
| filter_eq | duckdb | 191.116 | 269.42× | validated |
| filter_eq | postgres | 508.741 | 717.17× | validated |
| filter_eq | mysql | 750.715 | 1058.28× | validated |
| filter_eq | mongo | 1094.026 | 1542.25× | validated |
| filter_eq | pandas | 272.025 | 383.47× | validated |
| text_substr | lin | 16.449 | 1.00× | validated |
| text_substr | sqlite | 327.743 | 19.92× | validated |
| text_substr | duckdb | 105.375 | 6.41× | validated |
| text_substr | postgres | 834.538 | 50.73× | validated |
| text_substr | mysql | 1406.882 | 85.53× | validated |
| text_substr | mongo | 3558.771 | 216.35× | validated |
| text_substr | pandas | 506.893 | 30.82× | validated |
| materialize | lin | 389.200 | 1.00× | validated |
| materialize | sqlite | 1911.614 | 4.91× | validated |
| materialize | duckdb | 1288.542 | 3.31× | validated |
| materialize | postgres | 1739.000 | 4.47× | validated |
| materialize | mysql | 10103.583 | 25.96× | validated |
| materialize | mongo | 5600.605 | 14.39× | validated |
| materialize | pandas | 2063.052 | 5.30× | validated |
| join_inner | lin | 1412.951 | 1.00× | validated |
| join_inner | sqlite | 4508.812 | 3.19× | validated |
| join_inner | duckdb | 2719.959 | 1.93× | validated |
| join_inner | postgres | 4019.209 | 2.84× | validated |
| join_inner | mysql | 20959.042 | 14.83× | validated |
| join_inner | mongo | 116959.605 | 82.78× | validated |
| join_inner | pandas | 4404.604 | 3.12× | validated |
| join_filter | lin | 683.375 | 1.00× | validated |
| join_filter | sqlite | 2326.709 | 3.40× | validated |
| join_filter | duckdb | 1442.945 | 2.11× | validated |
| join_filter | postgres | 2654.042 | 3.88× | validated |
| join_filter | mysql | 11362.646 | 16.63× | validated |
| join_filter | mongo | 65321.229 | 95.59× | validated |
| join_filter | pandas | 2529.146 | 3.70× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
