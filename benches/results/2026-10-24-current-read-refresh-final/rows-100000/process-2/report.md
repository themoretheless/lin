# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.993 | 8.86× | validated |
| point_get | duckdb | 39.450 | 352.07× | validated |
| point_get | postgres | 245.772 | 2193.41× | validated |
| point_get | mysql | 319.109 | 2847.92× | validated |
| point_get | mongo | 476.611 | 4253.56× | validated |
| point_get | pandas | 4.365 | 38.95× | validated |
| point_get | lin | 0.112 | 1.00× | validated |
| filter_eq | sqlite | 685.134 | 808.21× | validated |
| filter_eq | duckdb | 420.371 | 495.89× | validated |
| filter_eq | postgres | 2610.760 | 3079.77× | validated |
| filter_eq | mysql | 3503.979 | 4133.45× | validated |
| filter_eq | mongo | 5008.542 | 5908.30× | validated |
| filter_eq | pandas | 2225.052 | 2624.77× | validated |
| filter_eq | lin | 0.848 | 1.00× | validated |
| text_substr | sqlite | 3563.084 | 17.14× | validated |
| text_substr | duckdb | 469.856 | 2.26× | validated |
| text_substr | postgres | 5670.917 | 27.28× | validated |
| text_substr | mysql | 9680.291 | 46.57× | validated |
| text_substr | mongo | 27811.584 | 133.80× | validated |
| text_substr | pandas | 4317.833 | 20.77× | validated |
| text_substr | lin | 207.864 | 1.00× | validated |
| materialize | sqlite | 20782.584 | 4.17× | validated |
| materialize | duckdb | 12343.083 | 2.47× | validated |
| materialize | postgres | 18220.250 | 3.65× | validated |
| materialize | mysql | 84756.250 | 16.99× | validated |
| materialize | mongo | 42705.312 | 8.56× | validated |
| materialize | pandas | 16486.562 | 3.31× | validated |
| materialize | lin | 4988.105 | 1.00× | validated |
| join_inner | sqlite | 52152.500 | 3.31× | validated |
| join_inner | duckdb | 29064.666 | 1.85× | validated |
| join_inner | postgres | 41040.416 | 2.61× | validated |
| join_inner | mysql | 165376.084 | 10.51× | validated |
| join_inner | mongo | 1098161.771 | 69.80× | validated |
| join_inner | pandas | 35392.250 | 2.25× | validated |
| join_inner | lin | 15733.062 | 1.00× | validated |
| join_filter | sqlite | 24896.188 | 3.55× | validated |
| join_filter | duckdb | 14283.896 | 2.03× | validated |
| join_filter | postgres | 25433.666 | 3.62× | validated |
| join_filter | mysql | 89178.104 | 12.70× | validated |
| join_filter | mongo | 567341.708 | 80.81× | validated |
| join_filter | pandas | 18000.500 | 2.56× | validated |
| join_filter | lin | 7020.771 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
