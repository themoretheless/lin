# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.113 | 1.00× | validated |
| point_get | sqlite | 0.980 | 8.63× | validated |
| point_get | duckdb | 38.676 | 340.88× | validated |
| point_get | postgres | 274.201 | 2416.71× | validated |
| point_get | mysql | 428.869 | 3779.90× | validated |
| point_get | mongo | 1650.896 | 14550.41× | validated |
| point_get | pandas | 4.425 | 39.00× | validated |
| filter_eq | lin | 0.836 | 1.00× | validated |
| filter_eq | sqlite | 677.155 | 809.85× | validated |
| filter_eq | duckdb | 444.244 | 531.30× | validated |
| filter_eq | postgres | 2871.625 | 3434.36× | validated |
| filter_eq | mysql | 3430.229 | 4102.43× | validated |
| filter_eq | mongo | 9424.208 | 11271.01× | validated |
| filter_eq | pandas | 2254.562 | 2696.37× | validated |
| text_substr | lin | 202.565 | 1.00× | validated |
| text_substr | sqlite | 3242.604 | 16.01× | validated |
| text_substr | duckdb | 479.652 | 2.37× | validated |
| text_substr | postgres | 5335.792 | 26.34× | validated |
| text_substr | mysql | 9629.229 | 47.54× | validated |
| text_substr | mongo | 46182.938 | 227.99× | validated |
| text_substr | pandas | 4438.729 | 21.91× | validated |
| materialize | lin | 5432.791 | 1.00× | validated |
| materialize | sqlite | 20201.646 | 3.72× | validated |
| materialize | duckdb | 12525.646 | 2.31× | validated |
| materialize | postgres | 14729.354 | 2.71× | validated |
| materialize | mysql | 85367.125 | 15.71× | validated |
| materialize | mongo | 76622.708 | 14.10× | validated |
| materialize | pandas | 17184.979 | 3.16× | validated |
| join_inner | lin | 15902.583 | 1.00× | validated |
| join_inner | sqlite | 50941.271 | 3.20× | validated |
| join_inner | duckdb | 29255.375 | 1.84× | validated |
| join_inner | postgres | 54620.229 | 3.43× | validated |
| join_inner | mysql | 303301.541 | 19.07× | validated |
| join_inner | mongo | 1146719.271 | 72.11× | validated |
| join_inner | pandas | 36146.584 | 2.27× | validated |
| join_filter | lin | 7522.605 | 1.00× | validated |
| join_filter | sqlite | 25360.896 | 3.37× | validated |
| join_filter | duckdb | 14713.500 | 1.96× | validated |
| join_filter | postgres | 22187.459 | 2.95× | validated |
| join_filter | mysql | 272361.896 | 36.21× | validated |
| join_filter | mongo | 590055.625 | 78.44× | validated |
| join_filter | pandas | 18565.500 | 2.47× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
