# Validated read API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.114 | 1.00× | validated |
| point_get | sqlite | 0.966 | 8.44× | validated |
| point_get | duckdb | 41.178 | 359.83× | validated |
| point_get | postgres | 263.750 | 2304.75× | validated |
| point_get | mysql | 337.304 | 2947.49× | validated |
| point_get | mongo | 446.279 | 3899.76× | validated |
| point_get | pandas | 4.556 | 39.81× | validated |
| filter_eq | lin | 0.695 | 1.00× | validated |
| filter_eq | sqlite | 68.358 | 98.34× | validated |
| filter_eq | duckdb | 194.461 | 279.75× | validated |
| filter_eq | postgres | 489.144 | 703.69× | validated |
| filter_eq | mysql | 673.027 | 968.23× | validated |
| filter_eq | mongo | 1053.026 | 1514.90× | validated |
| filter_eq | pandas | 270.822 | 389.61× | validated |
| text_substr | lin | 16.377 | 1.00× | validated |
| text_substr | sqlite | 332.875 | 20.33× | validated |
| text_substr | duckdb | 105.375 | 6.43× | validated |
| text_substr | postgres | 811.316 | 49.54× | validated |
| text_substr | mysql | 1379.618 | 84.24× | validated |
| text_substr | mongo | 3558.771 | 217.31× | validated |
| text_substr | pandas | 493.273 | 30.12× | validated |
| materialize | lin | 389.200 | 1.00× | validated |
| materialize | sqlite | 1910.386 | 4.91× | validated |
| materialize | duckdb | 1288.542 | 3.31× | validated |
| materialize | postgres | 1665.625 | 4.28× | validated |
| materialize | mysql | 10103.583 | 25.96× | validated |
| materialize | mongo | 5597.542 | 14.38× | validated |
| materialize | pandas | 1989.625 | 5.11× | validated |
| join_inner | lin | 1371.854 | 1.00× | validated |
| join_inner | sqlite | 4493.249 | 3.28× | validated |
| join_inner | duckdb | 2764.208 | 2.01× | validated |
| join_inner | postgres | 4019.209 | 2.93× | validated |
| join_inner | mysql | 20937.416 | 15.26× | validated |
| join_inner | mongo | 116944.979 | 85.25× | validated |
| join_inner | pandas | 4210.542 | 3.07× | validated |
| join_filter | lin | 683.375 | 1.00× | validated |
| join_filter | sqlite | 2361.209 | 3.46× | validated |
| join_filter | duckdb | 1488.424 | 2.18× | validated |
| join_filter | postgres | 2604.916 | 3.81× | validated |
| join_filter | mysql | 11178.229 | 16.36× | validated |
| join_filter | mongo | 60047.041 | 87.87× | validated |
| join_filter | pandas | 2466.552 | 3.61× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
