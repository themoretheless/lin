# Validated read API benchmark

Rows: 1000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.128 | 1.00× | validated |
| point_get | sqlite | 0.921 | 7.18× | validated |
| point_get | duckdb | 39.532 | 308.14× | validated |
| point_get | postgres | 311.101 | 2424.91× | validated |
| point_get | mysql | 310.268 | 2418.42× | validated |
| point_get | mongo | 416.058 | 3243.01× | validated |
| point_get | pandas | 5.098 | 39.74× | validated |
| filter_eq | lin | 0.683 | 1.00× | validated |
| filter_eq | sqlite | 7.485 | 10.95× | validated |
| filter_eq | duckdb | 137.609 | 201.34× | validated |
| filter_eq | postgres | 327.658 | 479.41× | validated |
| filter_eq | mysql | 344.021 | 503.35× | validated |
| filter_eq | mongo | 488.246 | 714.37× | validated |
| filter_eq | pandas | 55.386 | 81.04× | validated |
| text_substr | lin | 1.674 | 1.00× | validated |
| text_substr | sqlite | 32.151 | 19.21× | validated |
| text_substr | duckdb | 58.319 | 34.84× | validated |
| text_substr | postgres | 344.536 | 205.81× | validated |
| text_substr | mysql | 401.724 | 239.97× | validated |
| text_substr | mongo | 721.402 | 430.94× | validated |
| text_substr | pandas | 75.199 | 44.92× | validated |
| materialize | lin | 42.273 | 1.00× | validated |
| materialize | sqlite | 163.335 | 3.86× | validated |
| materialize | duckdb | 295.664 | 6.99× | validated |
| materialize | postgres | 416.090 | 9.84× | validated |
| materialize | mysql | 1184.323 | 28.02× | validated |
| materialize | mongo | 1207.016 | 28.55× | validated |
| materialize | pandas | 374.283 | 8.85× | validated |
| join_inner | lin | 129.316 | 1.00× | validated |
| join_inner | sqlite | 385.965 | 2.98× | validated |
| join_inner | duckdb | 352.240 | 2.72× | validated |
| join_inner | postgres | 633.141 | 4.90× | validated |
| join_inner | mysql | 2518.625 | 19.48× | validated |
| join_inner | mongo | 13411.417 | 103.71× | validated |
| join_inner | pandas | 708.670 | 5.48× | validated |
| join_filter | lin | 67.504 | 1.00× | validated |
| join_filter | sqlite | 198.024 | 2.93× | validated |
| join_filter | duckdb | 248.113 | 3.68× | validated |
| join_filter | postgres | 494.296 | 7.32× | validated |
| join_filter | mysql | 1602.188 | 23.73× | validated |
| join_filter | mongo | 7139.792 | 105.77× | validated |
| join_filter | pandas | 626.684 | 9.28× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
