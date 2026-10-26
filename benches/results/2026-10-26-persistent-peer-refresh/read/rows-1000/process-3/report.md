# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 42.350 | 347.50× | validated |
| point_get | postgres | 305.130 | 2503.68× | validated |
| point_get | mysql | 354.160 | 2905.98× | validated |
| point_get | mongo | 467.445 | 3835.51× | validated |
| point_get | pandas | 4.505 | 36.97× | validated |
| point_get | lin | 0.122 | 1.00× | validated |
| point_get | sqlite | 0.957 | 7.85× | validated |
| filter_eq | duckdb | 142.852 | 217.88× | validated |
| filter_eq | postgres | 322.208 | 491.44× | validated |
| filter_eq | mysql | 384.936 | 587.11× | validated |
| filter_eq | mongo | 554.443 | 845.65× | validated |
| filter_eq | pandas | 57.965 | 88.41× | validated |
| filter_eq | lin | 0.656 | 1.00× | validated |
| filter_eq | sqlite | 7.580 | 11.56× | validated |
| text_substr | duckdb | 63.675 | 35.64× | validated |
| text_substr | postgres | 367.233 | 205.55× | validated |
| text_substr | mysql | 443.031 | 247.98× | validated |
| text_substr | mongo | 841.976 | 471.27× | validated |
| text_substr | pandas | 75.874 | 42.47× | validated |
| text_substr | lin | 1.787 | 1.00× | validated |
| text_substr | sqlite | 32.594 | 18.24× | validated |
| materialize | duckdb | 299.436 | 7.70× | validated |
| materialize | postgres | 481.815 | 12.40× | validated |
| materialize | mysql | 1262.035 | 32.47× | validated |
| materialize | mongo | 1398.472 | 35.98× | validated |
| materialize | pandas | 410.144 | 10.55× | validated |
| materialize | lin | 38.870 | 1.00× | validated |
| materialize | sqlite | 172.290 | 4.43× | validated |
| join_inner | duckdb | 371.827 | 2.77× | validated |
| join_inner | postgres | 685.551 | 5.11× | validated |
| join_inner | mysql | 2336.677 | 17.43× | validated |
| join_inner | mongo | 13201.979 | 98.49× | validated |
| join_inner | pandas | 801.757 | 5.98× | validated |
| join_inner | lin | 134.047 | 1.00× | validated |
| join_inner | sqlite | 405.958 | 3.03× | validated |
| join_filter | duckdb | 264.256 | 3.91× | validated |
| join_filter | postgres | 533.891 | 7.91× | validated |
| join_filter | mysql | 1434.139 | 21.24× | validated |
| join_filter | mongo | 7562.688 | 112.02× | validated |
| join_filter | pandas | 680.009 | 10.07× | validated |
| join_filter | lin | 67.510 | 1.00× | validated |
| join_filter | sqlite | 205.783 | 3.05× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
