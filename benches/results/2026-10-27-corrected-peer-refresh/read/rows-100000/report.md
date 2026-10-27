# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.116 | 1.00× | validated |
| point_get | sqlite | 0.972 | 8.38× | validated |
| point_get | duckdb | 40.326 | 347.74× | validated |
| point_get | postgres | 249.565 | 2152.08× | validated |
| point_get | mysql | 346.361 | 2986.78× | validated |
| point_get | mongo | 485.000 | 4182.31× | validated |
| point_get | pandas | 4.441 | 38.29× | validated |
| filter_eq | lin | 0.864 | 1.00× | validated |
| filter_eq | sqlite | 672.902 | 778.98× | validated |
| filter_eq | duckdb | 428.421 | 495.96× | validated |
| filter_eq | postgres | 2405.749 | 2785.01× | validated |
| filter_eq | mysql | 3468.604 | 4015.42× | validated |
| filter_eq | mongo | 5168.812 | 5983.66× | validated |
| filter_eq | pandas | 2264.365 | 2621.33× | validated |
| text_substr | lin | 204.750 | 1.00× | validated |
| text_substr | sqlite | 3489.271 | 17.04× | validated |
| text_substr | duckdb | 475.908 | 2.32× | validated |
| text_substr | postgres | 5174.605 | 25.27× | validated |
| text_substr | mysql | 9835.208 | 48.04× | validated |
| text_substr | mongo | 27114.834 | 132.43× | validated |
| text_substr | pandas | 4847.083 | 23.67× | validated |
| materialize | lin | 5701.562 | 1.00× | validated |
| materialize | sqlite | 20152.166 | 3.53× | validated |
| materialize | duckdb | 12589.209 | 2.21× | validated |
| materialize | postgres | 17802.250 | 3.12× | validated |
| materialize | mysql | 84143.833 | 14.76× | validated |
| materialize | mongo | 43341.709 | 7.60× | validated |
| materialize | pandas | 17892.958 | 3.14× | validated |
| join_inner | lin | 16577.000 | 1.00× | validated |
| join_inner | sqlite | 52311.041 | 3.16× | validated |
| join_inner | duckdb | 29772.125 | 1.80× | validated |
| join_inner | postgres | 47305.979 | 2.85× | validated |
| join_inner | mysql | 167935.188 | 10.13× | validated |
| join_inner | mongo | 1126935.938 | 67.98× | validated |
| join_inner | pandas | 37379.125 | 2.25× | validated |
| join_filter | lin | 8219.084 | 1.00× | validated |
| join_filter | sqlite | 26343.521 | 3.21× | validated |
| join_filter | duckdb | 14471.750 | 1.76× | validated |
| join_filter | postgres | 22125.980 | 2.69× | validated |
| join_filter | mysql | 90482.605 | 11.01× | validated |
| join_filter | mongo | 573060.000 | 69.72× | validated |
| join_filter | pandas | 19782.042 | 2.41× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
