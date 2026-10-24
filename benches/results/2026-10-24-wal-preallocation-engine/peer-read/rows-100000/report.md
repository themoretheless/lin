# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.113 | 1.00× | validated |
| point_get | sqlite | 0.962 | 8.50× | validated |
| point_get | duckdb | 38.676 | 341.96× | validated |
| point_get | postgres | 274.201 | 2424.36× | validated |
| point_get | mysql | 344.743 | 3048.07× | validated |
| point_get | mongo | 1650.896 | 14596.51× | validated |
| point_get | pandas | 4.425 | 39.13× | validated |
| filter_eq | lin | 0.832 | 1.00× | validated |
| filter_eq | sqlite | 672.961 | 809.24× | validated |
| filter_eq | duckdb | 432.581 | 520.18× | validated |
| filter_eq | postgres | 2871.625 | 3453.16× | validated |
| filter_eq | mysql | 3447.291 | 4145.41× | validated |
| filter_eq | mongo | 9424.208 | 11332.72× | validated |
| filter_eq | pandas | 2254.562 | 2711.14× | validated |
| text_substr | lin | 202.565 | 1.00× | validated |
| text_substr | sqlite | 3263.229 | 16.11× | validated |
| text_substr | duckdb | 471.027 | 2.33× | validated |
| text_substr | postgres | 5335.792 | 26.34× | validated |
| text_substr | mysql | 9629.229 | 47.54× | validated |
| text_substr | mongo | 46182.938 | 227.99× | validated |
| text_substr | pandas | 4483.333 | 22.13× | validated |
| materialize | lin | 5147.499 | 1.00× | validated |
| materialize | sqlite | 20033.062 | 3.89× | validated |
| materialize | duckdb | 12525.646 | 2.43× | validated |
| materialize | postgres | 14729.354 | 2.86× | validated |
| materialize | mysql | 85367.125 | 16.58× | validated |
| materialize | mongo | 43776.459 | 8.50× | validated |
| materialize | pandas | 16850.458 | 3.27× | validated |
| join_inner | lin | 15630.292 | 1.00× | validated |
| join_inner | sqlite | 50363.541 | 3.22× | validated |
| join_inner | duckdb | 29255.375 | 1.87× | validated |
| join_inner | postgres | 54620.229 | 3.49× | validated |
| join_inner | mysql | 303301.541 | 19.40× | validated |
| join_inner | mongo | 1146719.271 | 73.37× | validated |
| join_inner | pandas | 36069.021 | 2.31× | validated |
| join_filter | lin | 7522.605 | 1.00× | validated |
| join_filter | sqlite | 25107.334 | 3.34× | validated |
| join_filter | duckdb | 14070.000 | 1.87× | validated |
| join_filter | postgres | 22187.459 | 2.95× | validated |
| join_filter | mysql | 272361.896 | 36.21× | validated |
| join_filter | mongo | 583477.479 | 77.56× | validated |
| join_filter | pandas | 18565.500 | 2.47× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
