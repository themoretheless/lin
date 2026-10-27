# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.972 | 8.38× | validated |
| point_get | duckdb | 38.628 | 333.10× | validated |
| point_get | postgres | 249.565 | 2152.08× | validated |
| point_get | mysql | 346.361 | 2986.78× | validated |
| point_get | mongo | 477.164 | 4114.74× | validated |
| point_get | pandas | 4.441 | 38.29× | validated |
| point_get | lin | 0.116 | 1.00× | validated |
| filter_eq | sqlite | 679.259 | 783.50× | validated |
| filter_eq | duckdb | 438.516 | 505.81× | validated |
| filter_eq | postgres | 2405.749 | 2774.94× | validated |
| filter_eq | mysql | 3401.313 | 3923.29× | validated |
| filter_eq | mongo | 5116.250 | 5901.40× | validated |
| filter_eq | pandas | 2264.365 | 2611.86× | validated |
| filter_eq | lin | 0.867 | 1.00× | validated |
| text_substr | sqlite | 3455.979 | 16.88× | validated |
| text_substr | duckdb | 491.000 | 2.40× | validated |
| text_substr | postgres | 5098.583 | 24.91× | validated |
| text_substr | mysql | 9793.834 | 47.84× | validated |
| text_substr | mongo | 27103.708 | 132.40× | validated |
| text_substr | pandas | 4477.667 | 21.87× | validated |
| text_substr | lin | 204.715 | 1.00× | validated |
| materialize | sqlite | 20152.166 | 3.69× | validated |
| materialize | duckdb | 12555.291 | 2.30× | validated |
| materialize | postgres | 17953.896 | 3.28× | validated |
| materialize | mysql | 83378.542 | 15.26× | validated |
| materialize | mongo | 43767.730 | 8.01× | validated |
| materialize | pandas | 17892.958 | 3.27× | validated |
| materialize | lin | 5465.583 | 1.00× | validated |
| join_inner | sqlite | 52335.687 | 3.23× | validated |
| join_inner | duckdb | 31008.896 | 1.91× | validated |
| join_inner | postgres | 47467.812 | 2.93× | validated |
| join_inner | mysql | 167752.271 | 10.35× | validated |
| join_inner | mongo | 1126935.938 | 69.50× | validated |
| join_inner | pandas | 37746.084 | 2.33× | validated |
| join_inner | lin | 16215.250 | 1.00× | validated |
| join_filter | sqlite | 25798.146 | 3.24× | validated |
| join_filter | duckdb | 14342.291 | 1.80× | validated |
| join_filter | postgres | 22125.980 | 2.78× | validated |
| join_filter | mysql | 90076.292 | 11.32× | validated |
| join_filter | mongo | 573060.000 | 72.04× | validated |
| join_filter | pandas | 19859.250 | 2.50× | validated |
| join_filter | lin | 7954.251 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
