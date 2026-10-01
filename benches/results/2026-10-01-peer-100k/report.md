# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.236 | 1.00× | validated |
| point_get | sqlite | 1.034 | 4.38× | validated |
| point_get | duckdb | 43.834 | 185.73× | validated |
| point_get | postgres | 295.713 | 1252.96× | validated |
| point_get | mysql | 360.752 | 1528.53× | validated |
| point_get | mongo | 391.441 | 1658.56× | validated |
| point_get | pandas | 4.673 | 19.80× | validated |
| filter_eq | lin | 0.780 | 1.00× | validated |
| filter_eq | sqlite | 719.309 | 922.69× | validated |
| filter_eq | duckdb | 450.562 | 577.96× | validated |
| filter_eq | postgres | 2940.312 | 3771.68× | validated |
| filter_eq | mysql | 3577.854 | 4589.48× | validated |
| filter_eq | mongo | 4290.708 | 5503.90× | validated |
| filter_eq | pandas | 2337.635 | 2998.60× | validated |
| text_substr | lin | 855.437 | 1.00× | validated |
| text_substr | sqlite | 3576.313 | 4.18× | validated |
| text_substr | duckdb | 488.704 | 0.57× | validated |
| text_substr | postgres | 5813.646 | 6.80× | validated |
| text_substr | mysql | 10051.729 | 11.75× | validated |
| text_substr | mongo | 24075.541 | 28.14× | validated |
| text_substr | pandas | 5030.750 | 5.88× | validated |
| materialize | lin | 6361.042 | 1.00× | validated |
| materialize | sqlite | 21355.959 | 3.36× | validated |
| materialize | duckdb | 12730.792 | 2.00× | validated |
| materialize | postgres | 15852.146 | 2.49× | validated |
| materialize | mysql | 87048.500 | 13.68× | validated |
| materialize | mongo | 46197.105 | 7.26× | validated |
| materialize | pandas | 18517.750 | 2.91× | validated |
| join_inner | lin | 14663.938 | 1.00× | validated |
| join_inner | sqlite | 53347.709 | 3.64× | validated |
| join_inner | duckdb | 29555.145 | 2.02× | validated |
| join_inner | postgres | 41862.126 | 2.85× | validated |
| join_inner | mysql | 177420.812 | 12.10× | validated |
| join_inner | mongo | 1184294.374 | 80.76× | validated |
| join_inner | pandas | 37337.396 | 2.55× | validated |
| join_filter | lin | 7559.542 | 1.00× | validated |
| join_filter | sqlite | 26567.791 | 3.51× | validated |
| join_filter | duckdb | 14352.792 | 1.90× | validated |
| join_filter | postgres | 22274.188 | 2.95× | validated |
| join_filter | mysql | 95286.917 | 12.60× | validated |
| join_filter | mongo | 597402.688 | 79.03× | validated |
| join_filter | pandas | 19291.834 | 2.55× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
