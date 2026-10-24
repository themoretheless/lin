# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 37.394 | 326.91× | validated |
| point_get | postgres | 306.118 | 2676.15× | validated |
| point_get | mysql | 330.182 | 2886.52× | validated |
| point_get | mongo | 413.140 | 3611.75× | validated |
| point_get | pandas | 4.491 | 39.26× | validated |
| point_get | lin | 0.114 | 1.00× | validated |
| point_get | sqlite | 0.950 | 8.30× | validated |
| filter_eq | duckdb | 139.037 | 216.98× | validated |
| filter_eq | postgres | 325.060 | 507.30× | validated |
| filter_eq | mysql | 363.703 | 567.60× | validated |
| filter_eq | mongo | 479.549 | 748.40× | validated |
| filter_eq | pandas | 56.090 | 87.53× | validated |
| filter_eq | lin | 0.641 | 1.00× | validated |
| filter_eq | sqlite | 7.286 | 11.37× | validated |
| text_substr | duckdb | 65.938 | 38.31× | validated |
| text_substr | postgres | 349.002 | 202.78× | validated |
| text_substr | mysql | 401.854 | 233.49× | validated |
| text_substr | mongo | 694.910 | 403.76× | validated |
| text_substr | pandas | 74.278 | 43.16× | validated |
| text_substr | lin | 1.721 | 1.00× | validated |
| text_substr | sqlite | 31.505 | 18.31× | validated |
| materialize | duckdb | 289.214 | 7.70× | validated |
| materialize | postgres | 440.242 | 11.73× | validated |
| materialize | mysql | 1195.359 | 31.84× | validated |
| materialize | mongo | 1220.812 | 32.52× | validated |
| materialize | pandas | 364.557 | 9.71× | validated |
| materialize | lin | 37.540 | 1.00× | validated |
| materialize | sqlite | 172.263 | 4.59× | validated |
| join_inner | duckdb | 347.960 | 2.72× | validated |
| join_inner | postgres | 665.881 | 5.20× | validated |
| join_inner | mysql | 2206.375 | 17.25× | validated |
| join_inner | mongo | 12432.104 | 97.18× | validated |
| join_inner | pandas | 702.736 | 5.49× | validated |
| join_inner | lin | 127.932 | 1.00× | validated |
| join_inner | sqlite | 396.967 | 3.10× | validated |
| join_filter | duckdb | 242.702 | 3.78× | validated |
| join_filter | postgres | 501.611 | 7.82× | validated |
| join_filter | mysql | 1313.799 | 20.47× | validated |
| join_filter | mongo | 6688.437 | 104.22× | validated |
| join_filter | pandas | 628.833 | 9.80× | validated |
| join_filter | lin | 64.176 | 1.00× | validated |
| join_filter | sqlite | 208.661 | 3.25× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
