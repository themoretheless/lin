# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.113 | 1.00× | validated |
| point_get | sqlite | 0.906 | 8.01× | validated |
| point_get | duckdb | 39.882 | 352.59× | validated |
| point_get | postgres | 314.625 | 2781.57× | validated |
| point_get | mysql | 339.861 | 3004.68× | validated |
| point_get | mongo | 439.904 | 3889.15× | validated |
| point_get | pandas | 4.954 | 43.80× | validated |
| filter_eq | lin | 0.650 | 1.00× | validated |
| filter_eq | sqlite | 7.646 | 11.77× | validated |
| filter_eq | duckdb | 136.619 | 210.29× | validated |
| filter_eq | postgres | 331.835 | 510.79× | validated |
| filter_eq | mysql | 356.779 | 549.18× | validated |
| filter_eq | mongo | 495.447 | 762.63× | validated |
| filter_eq | pandas | 57.441 | 88.42× | validated |
| text_substr | lin | 2.161 | 1.00× | validated |
| text_substr | sqlite | 32.472 | 15.02× | validated |
| text_substr | duckdb | 60.049 | 27.78× | validated |
| text_substr | postgres | 325.143 | 150.44× | validated |
| text_substr | mysql | 417.849 | 193.34× | validated |
| text_substr | mongo | 778.924 | 360.40× | validated |
| text_substr | pandas | 76.459 | 35.38× | validated |
| materialize | lin | 41.511 | 1.00× | validated |
| materialize | sqlite | 169.163 | 4.08× | validated |
| materialize | duckdb | 298.010 | 7.18× | validated |
| materialize | postgres | 411.844 | 9.92× | validated |
| materialize | mysql | 2071.639 | 49.91× | validated |
| materialize | mongo | 1726.708 | 41.60× | validated |
| materialize | pandas | 377.285 | 9.09× | validated |
| join_inner | lin | 131.707 | 1.00× | validated |
| join_inner | sqlite | 392.236 | 2.98× | validated |
| join_inner | duckdb | 356.502 | 2.71× | validated |
| join_inner | postgres | 659.798 | 5.01× | validated |
| join_inner | mysql | 2317.729 | 17.60× | validated |
| join_inner | mongo | 13405.021 | 101.78× | validated |
| join_inner | pandas | 730.146 | 5.54× | validated |
| join_filter | lin | 66.530 | 1.00× | validated |
| join_filter | sqlite | 202.809 | 3.05× | validated |
| join_filter | duckdb | 251.407 | 3.78× | validated |
| join_filter | postgres | 493.973 | 7.42× | validated |
| join_filter | mysql | 2447.802 | 36.79× | validated |
| join_filter | mongo | 8313.688 | 124.96× | validated |
| join_filter | pandas | 626.708 | 9.42× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
