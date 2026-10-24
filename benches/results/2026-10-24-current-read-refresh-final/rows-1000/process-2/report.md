# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.888 | 7.84× | validated |
| point_get | duckdb | 39.557 | 349.03× | validated |
| point_get | postgres | 312.301 | 2755.59× | validated |
| point_get | mysql | 329.699 | 2909.11× | validated |
| point_get | mongo | 521.319 | 4599.88× | validated |
| point_get | pandas | 7.391 | 65.22× | validated |
| point_get | lin | 0.113 | 1.00× | validated |
| filter_eq | sqlite | 7.425 | 11.89× | validated |
| filter_eq | duckdb | 132.538 | 212.24× | validated |
| filter_eq | postgres | 329.077 | 526.97× | validated |
| filter_eq | mysql | 359.609 | 575.86× | validated |
| filter_eq | mongo | 495.352 | 793.24× | validated |
| filter_eq | pandas | 70.174 | 112.37× | validated |
| filter_eq | lin | 0.624 | 1.00× | validated |
| text_substr | sqlite | 30.231 | 18.05× | validated |
| text_substr | duckdb | 56.419 | 33.69× | validated |
| text_substr | postgres | 347.743 | 207.65× | validated |
| text_substr | mysql | 408.245 | 243.78× | validated |
| text_substr | mongo | 695.399 | 415.25× | validated |
| text_substr | pandas | 73.930 | 44.15× | validated |
| text_substr | lin | 1.675 | 1.00× | validated |
| materialize | sqlite | 162.030 | 4.11× | validated |
| materialize | duckdb | 293.505 | 7.45× | validated |
| materialize | postgres | 416.659 | 10.57× | validated |
| materialize | mysql | 1222.312 | 31.01× | validated |
| materialize | mongo | 1196.882 | 30.37× | validated |
| materialize | pandas | 361.184 | 9.16× | validated |
| materialize | lin | 39.411 | 1.00× | validated |
| join_inner | sqlite | 383.369 | 3.06× | validated |
| join_inner | duckdb | 348.886 | 2.79× | validated |
| join_inner | postgres | 677.635 | 5.41× | validated |
| join_inner | mysql | 3017.583 | 24.11× | validated |
| join_inner | mongo | 13788.104 | 110.15× | validated |
| join_inner | pandas | 695.764 | 5.56× | validated |
| join_inner | lin | 125.172 | 1.00× | validated |
| join_filter | sqlite | 198.795 | 3.16× | validated |
| join_filter | duckdb | 246.071 | 3.91× | validated |
| join_filter | postgres | 500.026 | 7.94× | validated |
| join_filter | mysql | 1707.823 | 27.11× | validated |
| join_filter | mongo | 8211.729 | 130.35× | validated |
| join_filter | pandas | 623.008 | 9.89× | validated |
| join_filter | lin | 62.996 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
