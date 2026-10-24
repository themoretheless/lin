# Validated read API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.117 | 1.00× | validated |
| point_get | sqlite | 0.935 | 7.98× | validated |
| point_get | duckdb | 40.114 | 342.26× | validated |
| point_get | postgres | 232.035 | 1979.75× | validated |
| point_get | mysql | 331.050 | 2824.56× | validated |
| point_get | mongo | 409.023 | 3489.84× | validated |
| point_get | pandas | 4.593 | 39.19× | validated |
| filter_eq | lin | 0.664 | 1.00× | validated |
| filter_eq | sqlite | 69.449 | 104.56× | validated |
| filter_eq | duckdb | 184.388 | 277.60× | validated |
| filter_eq | postgres | 446.940 | 672.88× | validated |
| filter_eq | mysql | 631.077 | 950.10× | validated |
| filter_eq | mongo | 898.600 | 1352.86× | validated |
| filter_eq | pandas | 255.935 | 385.32× | validated |
| text_substr | lin | 16.032 | 1.00× | validated |
| text_substr | sqlite | 332.390 | 20.73× | validated |
| text_substr | duckdb | 95.725 | 5.97× | validated |
| text_substr | postgres | 728.629 | 45.45× | validated |
| text_substr | mysql | 1247.844 | 77.83× | validated |
| text_substr | mongo | 3012.479 | 187.90× | validated |
| text_substr | pandas | 472.223 | 29.45× | validated |
| materialize | lin | 366.471 | 1.00× | validated |
| materialize | sqlite | 1861.552 | 5.08× | validated |
| materialize | duckdb | 1221.156 | 3.33× | validated |
| materialize | postgres | 1862.260 | 5.08× | validated |
| materialize | mysql | 9619.208 | 26.25× | validated |
| materialize | mongo | 4981.750 | 13.59× | validated |
| materialize | pandas | 1829.896 | 4.99× | validated |
| join_inner | lin | 1262.646 | 1.00× | validated |
| join_inner | sqlite | 4519.333 | 3.58× | validated |
| join_inner | duckdb | 2619.562 | 2.07× | validated |
| join_inner | postgres | 3730.458 | 2.95× | validated |
| join_inner | mysql | 20354.917 | 16.12× | validated |
| join_inner | mongo | 115317.958 | 91.33× | validated |
| join_inner | pandas | 4033.084 | 3.19× | validated |
| join_filter | lin | 661.893 | 1.00× | validated |
| join_filter | sqlite | 2294.198 | 3.47× | validated |
| join_filter | duckdb | 1382.472 | 2.09× | validated |
| join_filter | postgres | 2164.073 | 3.27× | validated |
| join_filter | mysql | 10688.479 | 16.15× | validated |
| join_filter | mongo | 60267.979 | 91.05× | validated |
| join_filter | pandas | 2271.459 | 3.43× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
