# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.943 | 8.62× | validated |
| point_get | duckdb | 40.554 | 370.89× | validated |
| point_get | postgres | 250.352 | 2289.63× | validated |
| point_get | mysql | 331.050 | 3027.67× | validated |
| point_get | mongo | 409.023 | 3740.78× | validated |
| point_get | pandas | 5.684 | 51.98× | validated |
| point_get | lin | 0.109 | 1.00× | validated |
| filter_eq | sqlite | 69.603 | 108.32× | validated |
| filter_eq | duckdb | 184.388 | 286.95× | validated |
| filter_eq | postgres | 446.940 | 695.54× | validated |
| filter_eq | mysql | 640.768 | 997.18× | validated |
| filter_eq | mongo | 955.688 | 1487.27× | validated |
| filter_eq | pandas | 253.556 | 394.59× | validated |
| filter_eq | lin | 0.643 | 1.00× | validated |
| text_substr | sqlite | 330.158 | 19.67× | validated |
| text_substr | duckdb | 95.725 | 5.70× | validated |
| text_substr | postgres | 728.629 | 43.41× | validated |
| text_substr | mysql | 1216.557 | 72.47× | validated |
| text_substr | mongo | 3009.562 | 179.29× | validated |
| text_substr | pandas | 469.817 | 27.99× | validated |
| text_substr | lin | 16.786 | 1.00× | validated |
| materialize | sqlite | 1842.302 | 5.14× | validated |
| materialize | duckdb | 1234.924 | 3.45× | validated |
| materialize | postgres | 1862.260 | 5.20× | validated |
| materialize | mysql | 11315.000 | 31.59× | validated |
| materialize | mongo | 5261.083 | 14.69× | validated |
| materialize | pandas | 1829.896 | 5.11× | validated |
| materialize | lin | 358.229 | 1.00× | validated |
| join_inner | sqlite | 4519.333 | 3.62× | validated |
| join_inner | duckdb | 2619.562 | 2.10× | validated |
| join_inner | postgres | 3730.458 | 2.99× | validated |
| join_inner | mysql | 21282.041 | 17.05× | validated |
| join_inner | mongo | 115317.958 | 92.41× | validated |
| join_inner | pandas | 4300.375 | 3.45× | validated |
| join_inner | lin | 1247.943 | 1.00× | validated |
| join_filter | sqlite | 2331.073 | 3.71× | validated |
| join_filter | duckdb | 1382.472 | 2.20× | validated |
| join_filter | postgres | 2164.073 | 3.44× | validated |
| join_filter | mysql | 11885.041 | 18.91× | validated |
| join_filter | mongo | 64087.334 | 101.96× | validated |
| join_filter | pandas | 2388.323 | 3.80× | validated |
| join_filter | lin | 628.570 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
