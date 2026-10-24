# Validated read API benchmark

Rows: 1000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.113 | 1.00× | validated |
| point_get | sqlite | 0.887 | 7.81× | validated |
| point_get | duckdb | 37.373 | 329.45× | validated |
| point_get | postgres | 305.618 | 2694.10× | validated |
| point_get | mysql | 317.295 | 2797.04× | validated |
| point_get | mongo | 402.710 | 3550.00× | validated |
| point_get | pandas | 4.410 | 38.88× | validated |
| filter_eq | lin | 0.637 | 1.00× | validated |
| filter_eq | sqlite | 7.515 | 11.79× | validated |
| filter_eq | duckdb | 138.211 | 216.85× | validated |
| filter_eq | postgres | 325.060 | 510.01× | validated |
| filter_eq | mysql | 346.658 | 543.90× | validated |
| filter_eq | mongo | 479.549 | 752.41× | validated |
| filter_eq | pandas | 56.090 | 88.00× | validated |
| text_substr | lin | 1.840 | 1.00× | validated |
| text_substr | sqlite | 31.220 | 16.97× | validated |
| text_substr | duckdb | 65.066 | 35.36× | validated |
| text_substr | postgres | 347.681 | 188.95× | validated |
| text_substr | mysql | 397.861 | 216.22× | validated |
| text_substr | mongo | 694.910 | 377.66× | validated |
| text_substr | pandas | 74.278 | 40.37× | validated |
| materialize | lin | 37.986 | 1.00× | validated |
| materialize | sqlite | 165.203 | 4.35× | validated |
| materialize | duckdb | 289.848 | 7.63× | validated |
| materialize | postgres | 431.546 | 11.36× | validated |
| materialize | mysql | 1195.359 | 31.47× | validated |
| materialize | mongo | 1239.422 | 32.63× | validated |
| materialize | pandas | 370.330 | 9.75× | validated |
| join_inner | lin | 127.932 | 1.00× | validated |
| join_inner | sqlite | 388.573 | 3.04× | validated |
| join_inner | duckdb | 350.154 | 2.74× | validated |
| join_inner | postgres | 646.326 | 5.05× | validated |
| join_inner | mysql | 2206.375 | 17.25× | validated |
| join_inner | mongo | 12633.959 | 98.76× | validated |
| join_inner | pandas | 712.767 | 5.57× | validated |
| join_filter | lin | 65.457 | 1.00× | validated |
| join_filter | sqlite | 206.234 | 3.15× | validated |
| join_filter | duckdb | 242.702 | 3.71× | validated |
| join_filter | postgres | 495.599 | 7.57× | validated |
| join_filter | mysql | 1313.799 | 20.07× | validated |
| join_filter | mongo | 6688.437 | 102.18× | validated |
| join_filter | pandas | 628.833 | 9.61× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
