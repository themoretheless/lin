# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 43.834 | 185.73× | validated |
| point_get | postgres | 295.713 | 1252.96× | validated |
| point_get | mysql | 360.752 | 1528.53× | validated |
| point_get | mongo | 1320.174 | 5593.66× | validated |
| point_get | pandas | 4.673 | 19.80× | validated |
| point_get | lin | 0.236 | 1.00× | validated |
| point_get | sqlite | 1.034 | 4.38× | validated |
| filter_eq | duckdb | 453.389 | 581.58× | validated |
| filter_eq | postgres | 2636.354 | 3381.78× | validated |
| filter_eq | mysql | 3674.854 | 4713.91× | validated |
| filter_eq | mongo | 6572.958 | 8431.45× | validated |
| filter_eq | pandas | 2337.635 | 2998.60× | validated |
| filter_eq | lin | 0.780 | 1.00× | validated |
| filter_eq | sqlite | 719.309 | 922.69× | validated |
| text_substr | duckdb | 490.748 | 0.58× | validated |
| text_substr | postgres | 5571.042 | 6.55× | validated |
| text_substr | mysql | 10051.729 | 11.82× | validated |
| text_substr | mongo | 25801.750 | 30.35× | validated |
| text_substr | pandas | 4959.104 | 5.83× | validated |
| text_substr | lin | 850.274 | 1.00× | validated |
| text_substr | sqlite | 3576.313 | 4.21× | validated |
| materialize | duckdb | 12785.458 | 2.17× | validated |
| materialize | postgres | 18511.959 | 3.14× | validated |
| materialize | mysql | 87048.500 | 14.76× | validated |
| materialize | mongo | 46197.105 | 7.83× | validated |
| materialize | pandas | 17969.000 | 3.05× | validated |
| materialize | lin | 5898.417 | 1.00× | validated |
| materialize | sqlite | 21732.834 | 3.68× | validated |
| join_inner | duckdb | 30088.479 | 2.07× | validated |
| join_inner | postgres | 46402.271 | 3.20× | validated |
| join_inner | mysql | 180511.791 | 12.45× | validated |
| join_inner | mongo | 1184294.374 | 81.67× | validated |
| join_inner | pandas | 37456.812 | 2.58× | validated |
| join_inner | lin | 14500.854 | 1.00× | validated |
| join_inner | sqlite | 53347.709 | 3.68× | validated |
| join_filter | duckdb | 14067.750 | 1.86× | validated |
| join_filter | postgres | 22405.396 | 2.96× | validated |
| join_filter | mysql | 95286.917 | 12.60× | validated |
| join_filter | mongo | 606315.938 | 80.21× | validated |
| join_filter | pandas | 19384.730 | 2.56× | validated |
| join_filter | lin | 7559.542 | 1.00× | validated |
| join_filter | sqlite | 27307.730 | 3.61× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
