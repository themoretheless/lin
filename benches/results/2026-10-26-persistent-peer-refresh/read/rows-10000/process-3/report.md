# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 40.241 | 351.64× | validated |
| point_get | postgres | 259.674 | 2269.13× | validated |
| point_get | mysql | 322.890 | 2821.54× | validated |
| point_get | mongo | 422.090 | 3688.39× | validated |
| point_get | pandas | 4.576 | 39.99× | validated |
| point_get | lin | 0.114 | 1.00× | validated |
| point_get | sqlite | 0.966 | 8.44× | validated |
| filter_eq | duckdb | 196.366 | 283.88× | validated |
| filter_eq | postgres | 463.833 | 670.54× | validated |
| filter_eq | mysql | 650.723 | 940.71× | validated |
| filter_eq | mongo | 973.196 | 1406.89× | validated |
| filter_eq | pandas | 270.822 | 391.51× | validated |
| filter_eq | lin | 0.692 | 1.00× | validated |
| filter_eq | sqlite | 68.358 | 98.82× | validated |
| text_substr | duckdb | 104.259 | 6.54× | validated |
| text_substr | postgres | 778.260 | 48.81× | validated |
| text_substr | mysql | 1323.736 | 83.02× | validated |
| text_substr | mongo | 3740.646 | 234.60× | validated |
| text_substr | pandas | 492.481 | 30.89× | validated |
| text_substr | lin | 15.945 | 1.00× | validated |
| text_substr | sqlite | 337.824 | 21.19× | validated |
| materialize | duckdb | 1278.828 | 3.26× | validated |
| materialize | postgres | 1622.771 | 4.14× | validated |
| materialize | mysql | 9841.979 | 25.11× | validated |
| materialize | mongo | 5440.854 | 13.88× | validated |
| materialize | pandas | 1956.448 | 4.99× | validated |
| materialize | lin | 391.947 | 1.00× | validated |
| materialize | sqlite | 1910.386 | 4.87× | validated |
| join_inner | duckdb | 2764.208 | 2.04× | validated |
| join_inner | postgres | 4611.271 | 3.40× | validated |
| join_inner | mysql | 20795.062 | 15.33× | validated |
| join_inner | mongo | 116944.979 | 86.22× | validated |
| join_inner | pandas | 4210.542 | 3.10× | validated |
| join_inner | lin | 1356.431 | 1.00× | validated |
| join_inner | sqlite | 4493.249 | 3.31× | validated |
| join_filter | duckdb | 1488.424 | 2.20× | validated |
| join_filter | postgres | 2604.916 | 3.85× | validated |
| join_filter | mysql | 10906.166 | 16.11× | validated |
| join_filter | mongo | 60047.041 | 88.70× | validated |
| join_filter | pandas | 2397.635 | 3.54× | validated |
| join_filter | lin | 676.935 | 1.00× | validated |
| join_filter | sqlite | 2361.209 | 3.49× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
