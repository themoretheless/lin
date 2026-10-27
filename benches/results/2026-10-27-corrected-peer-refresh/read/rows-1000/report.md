# Validated read API benchmark

Rows: 1000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.117 | 1.00× | validated |
| point_get | sqlite | 0.919 | 7.82× | validated |
| point_get | duckdb | 39.923 | 340.03× | validated |
| point_get | postgres | 314.042 | 2674.73× | validated |
| point_get | mysql | 329.547 | 2806.79× | validated |
| point_get | mongo | 439.904 | 3746.72× | validated |
| point_get | pandas | 4.524 | 38.54× | validated |
| filter_eq | lin | 0.653 | 1.00× | validated |
| filter_eq | sqlite | 7.572 | 11.60× | validated |
| filter_eq | duckdb | 138.239 | 211.68× | validated |
| filter_eq | postgres | 331.835 | 508.13× | validated |
| filter_eq | mysql | 356.779 | 546.32× | validated |
| filter_eq | mongo | 499.285 | 764.53× | validated |
| filter_eq | pandas | 57.441 | 87.96× | validated |
| text_substr | lin | 2.139 | 1.00× | validated |
| text_substr | sqlite | 32.345 | 15.13× | validated |
| text_substr | duckdb | 61.891 | 28.94× | validated |
| text_substr | postgres | 354.500 | 165.77× | validated |
| text_substr | mysql | 401.797 | 187.89× | validated |
| text_substr | mongo | 742.256 | 347.09× | validated |
| text_substr | pandas | 74.884 | 35.02× | validated |
| materialize | lin | 39.446 | 1.00× | validated |
| materialize | sqlite | 169.163 | 4.29× | validated |
| materialize | duckdb | 298.533 | 7.57× | validated |
| materialize | postgres | 433.849 | 11.00× | validated |
| materialize | mysql | 1239.901 | 31.43× | validated |
| materialize | mongo | 1652.583 | 41.89× | validated |
| materialize | pandas | 372.283 | 9.44× | validated |
| join_inner | lin | 129.639 | 1.00× | validated |
| join_inner | sqlite | 392.236 | 3.03× | validated |
| join_inner | duckdb | 369.397 | 2.85× | validated |
| join_inner | postgres | 661.488 | 5.10× | validated |
| join_inner | mysql | 2317.729 | 17.88× | validated |
| join_inner | mongo | 13405.021 | 103.40× | validated |
| join_inner | pandas | 718.441 | 5.54× | validated |
| join_filter | lin | 64.833 | 1.00× | validated |
| join_filter | sqlite | 202.809 | 3.13× | validated |
| join_filter | duckdb | 255.363 | 3.94× | validated |
| join_filter | postgres | 497.056 | 7.67× | validated |
| join_filter | mysql | 1440.174 | 22.21× | validated |
| join_filter | mongo | 8313.688 | 128.23× | validated |
| join_filter | pandas | 626.708 | 9.67× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
