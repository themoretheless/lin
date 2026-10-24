# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.116 | 1.00× | validated |
| point_get | sqlite | 0.955 | 8.22× | validated |
| point_get | duckdb | 39.640 | 341.05× | validated |
| point_get | postgres | 254.801 | 2192.23× | validated |
| point_get | mysql | 348.835 | 3001.27× | validated |
| point_get | mongo | 426.852 | 3672.50× | validated |
| point_get | pandas | 4.522 | 38.90× | validated |
| filter_eq | lin | 0.658 | 1.00× | validated |
| filter_eq | sqlite | 68.524 | 104.16× | validated |
| filter_eq | duckdb | 189.576 | 288.18× | validated |
| filter_eq | postgres | 480.868 | 730.97× | validated |
| filter_eq | mysql | 650.235 | 988.42× | validated |
| filter_eq | mongo | 959.885 | 1459.12× | validated |
| filter_eq | pandas | 260.532 | 396.04× | validated |
| text_substr | lin | 17.022 | 1.00× | validated |
| text_substr | sqlite | 330.412 | 19.41× | validated |
| text_substr | duckdb | 104.013 | 6.11× | validated |
| text_substr | postgres | 758.167 | 44.54× | validated |
| text_substr | mysql | 1248.807 | 73.36× | validated |
| text_substr | mongo | 3148.833 | 184.98× | validated |
| text_substr | pandas | 499.544 | 29.35× | validated |
| materialize | lin | 407.595 | 1.00× | validated |
| materialize | sqlite | 1835.031 | 4.50× | validated |
| materialize | duckdb | 1262.844 | 3.10× | validated |
| materialize | postgres | 1578.479 | 3.87× | validated |
| materialize | mysql | 9746.479 | 23.91× | validated |
| materialize | mongo | 5256.646 | 12.90× | validated |
| materialize | pandas | 1837.688 | 4.51× | validated |
| join_inner | lin | 1315.799 | 1.00× | validated |
| join_inner | sqlite | 4522.895 | 3.44× | validated |
| join_inner | duckdb | 2522.041 | 1.92× | validated |
| join_inner | postgres | 3704.709 | 2.82× | validated |
| join_inner | mysql | 20772.104 | 15.79× | validated |
| join_inner | mongo | 113240.708 | 86.06× | validated |
| join_inner | pandas | 4067.646 | 3.09× | validated |
| join_filter | lin | 665.232 | 1.00× | validated |
| join_filter | sqlite | 2284.083 | 3.43× | validated |
| join_filter | duckdb | 1363.611 | 2.05× | validated |
| join_filter | postgres | 2296.573 | 3.45× | validated |
| join_filter | mysql | 11034.354 | 16.59× | validated |
| join_filter | mongo | 59465.728 | 89.39× | validated |
| join_filter | pandas | 2397.792 | 3.60× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
