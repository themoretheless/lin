# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.928 | 7.87× | validated |
| point_get | duckdb | 39.923 | 338.36× | validated |
| point_get | postgres | 302.263 | 2561.78× | validated |
| point_get | mysql | 315.950 | 2677.77× | validated |
| point_get | mongo | 467.133 | 3959.10× | validated |
| point_get | pandas | 4.524 | 38.35× | validated |
| point_get | lin | 0.118 | 1.00× | validated |
| filter_eq | sqlite | 7.524 | 11.52× | validated |
| filter_eq | duckdb | 138.239 | 211.68× | validated |
| filter_eq | postgres | 325.119 | 497.84× | validated |
| filter_eq | mysql | 343.343 | 525.75× | validated |
| filter_eq | mongo | 541.779 | 829.60× | validated |
| filter_eq | pandas | 57.911 | 88.68× | validated |
| filter_eq | lin | 0.653 | 1.00× | validated |
| text_substr | sqlite | 31.657 | 15.26× | validated |
| text_substr | duckdb | 61.891 | 29.83× | validated |
| text_substr | postgres | 354.500 | 170.88× | validated |
| text_substr | mysql | 401.797 | 193.68× | validated |
| text_substr | mongo | 713.417 | 343.89× | validated |
| text_substr | pandas | 73.315 | 35.34× | validated |
| text_substr | lin | 2.075 | 1.00× | validated |
| materialize | sqlite | 163.407 | 4.22× | validated |
| materialize | duckdb | 298.533 | 7.71× | validated |
| materialize | postgres | 433.849 | 11.21× | validated |
| materialize | mysql | 1181.151 | 30.51× | validated |
| materialize | mongo | 1613.424 | 41.68× | validated |
| materialize | pandas | 366.087 | 9.46× | validated |
| materialize | lin | 38.708 | 1.00× | validated |
| join_inner | sqlite | 392.005 | 3.07× | validated |
| join_inner | duckdb | 369.397 | 2.90× | validated |
| join_inner | postgres | 661.488 | 5.19× | validated |
| join_inner | mysql | 2304.552 | 18.08× | validated |
| join_inner | mongo | 13924.791 | 109.22× | validated |
| join_inner | pandas | 704.056 | 5.52× | validated |
| join_inner | lin | 127.496 | 1.00× | validated |
| join_filter | sqlite | 201.718 | 3.16× | validated |
| join_filter | duckdb | 255.363 | 4.00× | validated |
| join_filter | postgres | 497.056 | 7.79× | validated |
| join_filter | mysql | 1434.555 | 22.48× | validated |
| join_filter | mongo | 8530.084 | 133.65× | validated |
| join_filter | pandas | 625.914 | 9.81× | validated |
| join_filter | lin | 63.822 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
