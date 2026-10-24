# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.994 | 9.13× | validated |
| point_get | duckdb | 39.366 | 361.42× | validated |
| point_get | postgres | 249.988 | 2295.13× | validated |
| point_get | mysql | 332.653 | 3054.09× | validated |
| point_get | mongo | 399.015 | 3663.35× | validated |
| point_get | pandas | 4.337 | 39.82× | validated |
| point_get | lin | 0.109 | 1.00× | validated |
| filter_eq | sqlite | 691.720 | 818.72× | validated |
| filter_eq | duckdb | 418.953 | 495.87× | validated |
| filter_eq | postgres | 2284.177 | 2703.55× | validated |
| filter_eq | mysql | 3283.001 | 3885.76× | validated |
| filter_eq | mongo | 4989.979 | 5906.14× | validated |
| filter_eq | pandas | 2172.615 | 2571.51× | validated |
| filter_eq | lin | 0.845 | 1.00× | validated |
| text_substr | sqlite | 3338.979 | 15.77× | validated |
| text_substr | duckdb | 453.925 | 2.14× | validated |
| text_substr | postgres | 5256.000 | 24.82× | validated |
| text_substr | mysql | 9433.416 | 44.55× | validated |
| text_substr | mongo | 26116.209 | 123.32× | validated |
| text_substr | pandas | 4483.958 | 21.17× | validated |
| text_substr | lin | 211.767 | 1.00× | validated |
| materialize | sqlite | 20491.500 | 4.25× | validated |
| materialize | duckdb | 12207.959 | 2.53× | validated |
| materialize | postgres | 14636.416 | 3.04× | validated |
| materialize | mysql | 83758.688 | 17.39× | validated |
| materialize | mongo | 41798.770 | 8.68× | validated |
| materialize | pandas | 17136.750 | 3.56× | validated |
| materialize | lin | 4816.792 | 1.00× | validated |
| join_inner | sqlite | 52047.812 | 3.24× | validated |
| join_inner | duckdb | 28373.980 | 1.77× | validated |
| join_inner | postgres | 39464.021 | 2.46× | validated |
| join_inner | mysql | 166017.396 | 10.34× | validated |
| join_inner | mongo | 1118170.312 | 69.64× | validated |
| join_inner | pandas | 37419.104 | 2.33× | validated |
| join_inner | lin | 16056.583 | 1.00× | validated |
| join_filter | sqlite | 25525.916 | 3.38× | validated |
| join_filter | duckdb | 13355.854 | 1.77× | validated |
| join_filter | postgres | 21037.354 | 2.78× | validated |
| join_filter | mysql | 89701.792 | 11.87× | validated |
| join_filter | mongo | 586254.792 | 77.58× | validated |
| join_filter | pandas | 18966.479 | 2.51× | validated |
| join_filter | lin | 7556.875 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
