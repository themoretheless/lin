# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.115 | 1.00× | validated |
| point_get | sqlite | 1.018 | 8.88× | validated |
| point_get | duckdb | 39.822 | 347.30× | validated |
| point_get | postgres | 251.652 | 2194.76× | validated |
| point_get | mysql | 327.338 | 2854.85× | validated |
| point_get | mongo | 556.778 | 4855.88× | validated |
| point_get | pandas | 4.468 | 38.97× | validated |
| filter_eq | lin | 0.892 | 1.00× | validated |
| filter_eq | sqlite | 681.679 | 764.38× | validated |
| filter_eq | duckdb | 437.669 | 490.77× | validated |
| filter_eq | postgres | 2532.354 | 2839.58× | validated |
| filter_eq | mysql | 3318.438 | 3721.03× | validated |
| filter_eq | mongo | 4957.833 | 5559.32× | validated |
| filter_eq | pandas | 2319.927 | 2601.38× | validated |
| text_substr | lin | 210.702 | 1.00× | validated |
| text_substr | sqlite | 3484.958 | 16.54× | validated |
| text_substr | duckdb | 457.508 | 2.17× | validated |
| text_substr | postgres | 5066.938 | 24.05× | validated |
| text_substr | mysql | 9446.688 | 44.83× | validated |
| text_substr | mongo | 26317.730 | 124.90× | validated |
| text_substr | pandas | 4911.562 | 23.31× | validated |
| materialize | lin | 5860.438 | 1.00× | validated |
| materialize | sqlite | 20569.291 | 3.51× | validated |
| materialize | duckdb | 12115.312 | 2.07× | validated |
| materialize | postgres | 14473.646 | 2.47× | validated |
| materialize | mysql | 83190.688 | 14.20× | validated |
| materialize | mongo | 41400.104 | 7.06× | validated |
| materialize | pandas | 18245.521 | 3.11× | validated |
| join_inner | lin | 16794.125 | 1.00× | validated |
| join_inner | sqlite | 52062.395 | 3.10× | validated |
| join_inner | duckdb | 28519.645 | 1.70× | validated |
| join_inner | postgres | 38917.188 | 2.32× | validated |
| join_inner | mysql | 166505.438 | 9.91× | validated |
| join_inner | mongo | 1166586.667 | 69.46× | validated |
| join_inner | pandas | 36916.313 | 2.20× | validated |
| join_filter | lin | 8266.041 | 1.00× | validated |
| join_filter | sqlite | 26087.625 | 3.16× | validated |
| join_filter | duckdb | 13676.854 | 1.65× | validated |
| join_filter | postgres | 21356.896 | 2.58× | validated |
| join_filter | mysql | 90613.250 | 10.96× | validated |
| join_filter | mongo | 577422.833 | 69.85× | validated |
| join_filter | pandas | 19349.188 | 2.34× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
