# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.247 | 1.00× | validated |
| point_get | sqlite | 1.032 | 4.17× | validated |
| point_get | duckdb | 40.372 | 163.13× | validated |
| point_get | postgres | 229.497 | 927.33× | validated |
| point_get | mysql | 276.396 | 1116.84× | validated |
| point_get | mongo | 391.441 | 1581.70× | validated |
| point_get | pandas | 4.604 | 18.60× | validated |
| filter_eq | lin | 0.791 | 1.00× | validated |
| filter_eq | sqlite | 714.118 | 903.20× | validated |
| filter_eq | duckdb | 428.335 | 541.75× | validated |
| filter_eq | postgres | 3147.854 | 3981.33× | validated |
| filter_eq | mysql | 3577.854 | 4525.18× | validated |
| filter_eq | mongo | 4163.354 | 5265.71× | validated |
| filter_eq | pandas | 2289.010 | 2895.08× | validated |
| text_substr | lin | 861.612 | 1.00× | validated |
| text_substr | sqlite | 3532.041 | 4.10× | validated |
| text_substr | duckdb | 482.681 | 0.56× | validated |
| text_substr | postgres | 5959.646 | 6.92× | validated |
| text_substr | mysql | 10195.062 | 11.83× | validated |
| text_substr | mongo | 24075.541 | 27.94× | validated |
| text_substr | pandas | 5030.750 | 5.84× | validated |
| materialize | lin | 6560.020 | 1.00× | validated |
| materialize | sqlite | 21097.312 | 3.22× | validated |
| materialize | duckdb | 12730.792 | 1.94× | validated |
| materialize | postgres | 15852.146 | 2.42× | validated |
| materialize | mysql | 86260.125 | 13.15× | validated |
| materialize | mongo | 46523.041 | 7.09× | validated |
| materialize | pandas | 18517.750 | 2.82× | validated |
| join_inner | lin | 14663.938 | 1.00× | validated |
| join_inner | sqlite | 57772.438 | 3.94× | validated |
| join_inner | duckdb | 29555.145 | 2.02× | validated |
| join_inner | postgres | 41862.126 | 2.85× | validated |
| join_inner | mysql | 177420.812 | 12.10× | validated |
| join_inner | mongo | 1158453.292 | 79.00× | validated |
| join_inner | pandas | 36443.105 | 2.49× | validated |
| join_filter | lin | 7686.626 | 1.00× | validated |
| join_filter | sqlite | 26567.791 | 3.46× | validated |
| join_filter | duckdb | 14681.416 | 1.91× | validated |
| join_filter | postgres | 22026.104 | 2.87× | validated |
| join_filter | mysql | 99965.541 | 13.01× | validated |
| join_filter | mongo | 597046.709 | 77.67× | validated |
| join_filter | pandas | 19104.416 | 2.49× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
