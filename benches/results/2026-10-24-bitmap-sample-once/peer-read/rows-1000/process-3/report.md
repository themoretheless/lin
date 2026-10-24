# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 37.093 | 289.12× | validated |
| point_get | postgres | 311.101 | 2424.91× | validated |
| point_get | mysql | 310.268 | 2418.42× | validated |
| point_get | mongo | 396.340 | 3089.32× | validated |
| point_get | pandas | 4.397 | 34.27× | validated |
| point_get | lin | 0.128 | 1.00× | validated |
| point_get | sqlite | 0.933 | 7.27× | validated |
| filter_eq | duckdb | 138.941 | 201.15× | validated |
| filter_eq | postgres | 327.658 | 474.37× | validated |
| filter_eq | mysql | 344.021 | 498.06× | validated |
| filter_eq | mongo | 488.246 | 706.86× | validated |
| filter_eq | pandas | 55.324 | 80.10× | validated |
| filter_eq | lin | 0.691 | 1.00× | validated |
| filter_eq | sqlite | 7.485 | 10.84× | validated |
| text_substr | duckdb | 62.789 | 37.51× | validated |
| text_substr | postgres | 344.536 | 205.81× | validated |
| text_substr | mysql | 401.724 | 239.97× | validated |
| text_substr | mongo | 732.351 | 437.48× | validated |
| text_substr | pandas | 75.199 | 44.92× | validated |
| text_substr | lin | 1.674 | 1.00× | validated |
| text_substr | sqlite | 32.489 | 19.41× | validated |
| materialize | duckdb | 295.664 | 7.68× | validated |
| materialize | postgres | 425.617 | 11.06× | validated |
| materialize | mysql | 1184.323 | 30.78× | validated |
| materialize | mongo | 1159.938 | 30.14× | validated |
| materialize | pandas | 377.821 | 9.82× | validated |
| materialize | lin | 38.480 | 1.00× | validated |
| materialize | sqlite | 170.851 | 4.44× | validated |
| join_inner | duckdb | 352.240 | 2.72× | validated |
| join_inner | postgres | 633.141 | 4.88× | validated |
| join_inner | mysql | 2161.750 | 16.68× | validated |
| join_inner | mongo | 13330.854 | 102.83× | validated |
| join_inner | pandas | 722.153 | 5.57× | validated |
| join_inner | lin | 129.638 | 1.00× | validated |
| join_inner | sqlite | 399.776 | 3.08× | validated |
| join_filter | duckdb | 248.113 | 3.68× | validated |
| join_filter | postgres | 494.296 | 7.32× | validated |
| join_filter | mysql | 1288.368 | 19.09× | validated |
| join_filter | mongo | 7139.792 | 105.77× | validated |
| join_filter | pandas | 626.684 | 9.28× | validated |
| join_filter | lin | 67.504 | 1.00× | validated |
| join_filter | sqlite | 207.561 | 3.07× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
