# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.114 | 1.00× | validated |
| point_get | sqlite | 0.946 | 8.27× | validated |
| point_get | duckdb | 41.526 | 363.21× | validated |
| point_get | postgres | 243.040 | 2125.76× | validated |
| point_get | mysql | 339.316 | 2967.83× | validated |
| point_get | mongo | 579.878 | 5071.91× | validated |
| point_get | pandas | 4.307 | 37.67× | validated |
| filter_eq | lin | 0.856 | 1.00× | validated |
| filter_eq | sqlite | 666.336 | 778.47× | validated |
| filter_eq | duckdb | 428.421 | 500.52× | validated |
| filter_eq | postgres | 2440.219 | 2850.88× | validated |
| filter_eq | mysql | 3519.562 | 4111.86× | validated |
| filter_eq | mongo | 5168.812 | 6038.66× | validated |
| filter_eq | pandas | 2223.281 | 2597.43× | validated |
| text_substr | lin | 205.561 | 1.00× | validated |
| text_substr | sqlite | 3636.979 | 17.69× | validated |
| text_substr | duckdb | 475.908 | 2.32× | validated |
| text_substr | postgres | 5174.605 | 25.17× | validated |
| text_substr | mysql | 9835.208 | 47.85× | validated |
| text_substr | mongo | 27411.479 | 133.35× | validated |
| text_substr | pandas | 4847.083 | 23.58× | validated |
| materialize | lin | 5701.562 | 1.00× | validated |
| materialize | sqlite | 19888.104 | 3.49× | validated |
| materialize | duckdb | 12741.812 | 2.23× | validated |
| materialize | postgres | 14871.833 | 2.61× | validated |
| materialize | mysql | 84143.833 | 14.76× | validated |
| materialize | mongo | 43341.709 | 7.60× | validated |
| materialize | pandas | 17367.979 | 3.05× | validated |
| join_inner | lin | 16577.000 | 1.00× | validated |
| join_inner | sqlite | 52246.876 | 3.15× | validated |
| join_inner | duckdb | 29772.125 | 1.80× | validated |
| join_inner | postgres | 40562.709 | 2.45× | validated |
| join_inner | mysql | 167984.250 | 10.13× | validated |
| join_inner | mongo | 1113920.896 | 67.20× | validated |
| join_inner | pandas | 36974.584 | 2.23× | validated |
| join_filter | lin | 8445.166 | 1.00× | validated |
| join_filter | sqlite | 26343.521 | 3.12× | validated |
| join_filter | duckdb | 14517.166 | 1.72× | validated |
| join_filter | postgres | 21706.479 | 2.57× | validated |
| join_filter | mysql | 90710.708 | 10.74× | validated |
| join_filter | mongo | 569262.958 | 67.41× | validated |
| join_filter | pandas | 19319.417 | 2.29× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
