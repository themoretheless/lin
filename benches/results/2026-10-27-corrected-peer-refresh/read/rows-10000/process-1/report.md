# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.118 | 1.00× | validated |
| point_get | sqlite | 0.929 | 7.90× | validated |
| point_get | duckdb | 40.549 | 344.84× | validated |
| point_get | postgres | 244.930 | 2082.96× | validated |
| point_get | mysql | 266.653 | 2267.70× | validated |
| point_get | mongo | 420.079 | 3572.48× | validated |
| point_get | pandas | 4.442 | 37.78× | validated |
| filter_eq | lin | 0.667 | 1.00× | validated |
| filter_eq | sqlite | 66.070 | 99.08× | validated |
| filter_eq | duckdb | 186.919 | 280.30× | validated |
| filter_eq | postgres | 465.146 | 697.52× | validated |
| filter_eq | mysql | 582.268 | 873.15× | validated |
| filter_eq | mongo | 906.029 | 1358.65× | validated |
| filter_eq | pandas | 267.843 | 401.65× | validated |
| text_substr | lin | 19.172 | 1.00× | validated |
| text_substr | sqlite | 324.044 | 16.90× | validated |
| text_substr | duckdb | 100.026 | 5.22× | validated |
| text_substr | postgres | 736.767 | 38.43× | validated |
| text_substr | mysql | 1228.724 | 64.09× | validated |
| text_substr | mongo | 3093.709 | 161.37× | validated |
| text_substr | pandas | 488.873 | 25.50× | validated |
| materialize | lin | 385.641 | 1.00× | validated |
| materialize | sqlite | 1846.146 | 4.79× | validated |
| materialize | duckdb | 1244.708 | 3.23× | validated |
| materialize | postgres | 1858.646 | 4.82× | validated |
| materialize | mysql | 10101.813 | 26.19× | validated |
| materialize | mongo | 5519.167 | 14.31× | validated |
| materialize | pandas | 1868.604 | 4.85× | validated |
| join_inner | lin | 1338.472 | 1.00× | validated |
| join_inner | sqlite | 4451.333 | 3.33× | validated |
| join_inner | duckdb | 2610.062 | 1.95× | validated |
| join_inner | postgres | 4026.250 | 3.01× | validated |
| join_inner | mysql | 20398.750 | 15.24× | validated |
| join_inner | mongo | 121065.312 | 90.45× | validated |
| join_inner | pandas | 3909.312 | 2.92× | validated |
| join_filter | lin | 651.031 | 1.00× | validated |
| join_filter | sqlite | 2310.468 | 3.55× | validated |
| join_filter | duckdb | 1436.611 | 2.21× | validated |
| join_filter | postgres | 2270.812 | 3.49× | validated |
| join_filter | mysql | 11033.563 | 16.95× | validated |
| join_filter | mongo | 63193.292 | 97.07× | validated |
| join_filter | pandas | 2242.917 | 3.45× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
