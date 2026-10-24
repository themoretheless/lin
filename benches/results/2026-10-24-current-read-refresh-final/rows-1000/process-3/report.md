# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 34.167 | 288.67× | validated |
| point_get | postgres | 218.511 | 1846.15× | validated |
| point_get | mysql | 196.820 | 1662.89× | validated |
| point_get | mongo | 275.070 | 2324.00× | validated |
| point_get | pandas | 4.507 | 38.08× | validated |
| point_get | lin | 0.118 | 1.00× | validated |
| point_get | sqlite | 0.928 | 7.84× | validated |
| filter_eq | duckdb | 129.052 | 194.36× | validated |
| filter_eq | postgres | 224.537 | 338.16× | validated |
| filter_eq | mysql | 226.984 | 341.85× | validated |
| filter_eq | mongo | 363.701 | 547.75× | validated |
| filter_eq | pandas | 59.387 | 89.44× | validated |
| filter_eq | lin | 0.664 | 1.00× | validated |
| filter_eq | sqlite | 7.473 | 11.25× | validated |
| text_substr | duckdb | 58.478 | 35.65× | validated |
| text_substr | postgres | 316.772 | 193.11× | validated |
| text_substr | mysql | 277.957 | 169.45× | validated |
| text_substr | mongo | 617.193 | 376.26× | validated |
| text_substr | pandas | 75.495 | 46.02× | validated |
| text_substr | lin | 1.640 | 1.00× | validated |
| text_substr | sqlite | 32.361 | 19.73× | validated |
| materialize | duckdb | 291.861 | 7.53× | validated |
| materialize | postgres | 311.503 | 8.04× | validated |
| materialize | mysql | 1140.271 | 29.43× | validated |
| materialize | mongo | 971.696 | 25.08× | validated |
| materialize | pandas | 374.684 | 9.67× | validated |
| materialize | lin | 38.742 | 1.00× | validated |
| materialize | sqlite | 172.765 | 4.46× | validated |
| join_inner | duckdb | 365.405 | 2.79× | validated |
| join_inner | postgres | 538.440 | 4.11× | validated |
| join_inner | mysql | 3557.615 | 27.17× | validated |
| join_inner | mongo | 13778.792 | 105.25× | validated |
| join_inner | pandas | 724.375 | 5.53× | validated |
| join_inner | lin | 130.918 | 1.00× | validated |
| join_inner | sqlite | 399.406 | 3.05× | validated |
| join_filter | duckdb | 243.576 | 3.62× | validated |
| join_filter | postgres | 439.729 | 6.53× | validated |
| join_filter | mysql | 1194.562 | 17.74× | validated |
| join_filter | mongo | 8354.646 | 124.10× | validated |
| join_filter | pandas | 641.924 | 9.54× | validated |
| join_filter | lin | 67.320 | 1.00× | validated |
| join_filter | sqlite | 206.524 | 3.07× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
