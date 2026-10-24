# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.120 | 1.00× | validated |
| point_get | sqlite | 0.940 | 7.81× | validated |
| point_get | duckdb | 39.972 | 332.05× | validated |
| point_get | postgres | 321.753 | 2672.83× | validated |
| point_get | mysql | 347.277 | 2884.86× | validated |
| point_get | mongo | 398.158 | 3307.53× | validated |
| point_get | pandas | 4.784 | 39.74× | validated |
| filter_eq | lin | 0.682 | 1.00× | validated |
| filter_eq | sqlite | 7.525 | 11.03× | validated |
| filter_eq | duckdb | 136.216 | 199.61× | validated |
| filter_eq | postgres | 340.510 | 498.97× | validated |
| filter_eq | mysql | 364.205 | 533.69× | validated |
| filter_eq | mongo | 499.843 | 732.45× | validated |
| filter_eq | pandas | 55.536 | 81.38× | validated |
| text_substr | lin | 1.927 | 1.00× | validated |
| text_substr | sqlite | 32.283 | 16.76× | validated |
| text_substr | duckdb | 59.179 | 30.72× | validated |
| text_substr | postgres | 349.500 | 181.41× | validated |
| text_substr | mysql | 416.361 | 216.11× | validated |
| text_substr | mongo | 738.850 | 383.50× | validated |
| text_substr | pandas | 72.633 | 37.70× | validated |
| materialize | lin | 39.421 | 1.00× | validated |
| materialize | sqlite | 163.953 | 4.16× | validated |
| materialize | duckdb | 291.801 | 7.40× | validated |
| materialize | postgres | 455.664 | 11.56× | validated |
| materialize | mysql | 1218.844 | 30.92× | validated |
| materialize | mongo | 1284.368 | 32.58× | validated |
| materialize | pandas | 363.778 | 9.23× | validated |
| join_inner | lin | 130.820 | 1.00× | validated |
| join_inner | sqlite | 405.762 | 3.10× | validated |
| join_inner | duckdb | 362.021 | 2.77× | validated |
| join_inner | postgres | 685.191 | 5.24× | validated |
| join_inner | mysql | 2198.031 | 16.80× | validated |
| join_inner | mongo | 12545.603 | 95.90× | validated |
| join_inner | pandas | 706.236 | 5.40× | validated |
| join_filter | lin | 64.201 | 1.00× | validated |
| join_filter | sqlite | 202.403 | 3.15× | validated |
| join_filter | duckdb | 245.580 | 3.83× | validated |
| join_filter | postgres | 509.568 | 7.94× | validated |
| join_filter | mysql | 1383.139 | 21.54× | validated |
| join_filter | mongo | 6663.896 | 103.80× | validated |
| join_filter | pandas | 614.488 | 9.57× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
