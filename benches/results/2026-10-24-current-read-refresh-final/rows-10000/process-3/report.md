# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 43.013 | 365.51× | validated |
| point_get | postgres | 260.786 | 2216.08× | validated |
| point_get | mysql | 278.134 | 2363.50× | validated |
| point_get | mongo | 420.302 | 3571.59× | validated |
| point_get | pandas | 5.790 | 49.20× | validated |
| point_get | lin | 0.118 | 1.00× | validated |
| point_get | sqlite | 0.905 | 7.69× | validated |
| filter_eq | duckdb | 192.283 | 297.37× | validated |
| filter_eq | postgres | 478.421 | 739.88× | validated |
| filter_eq | mysql | 675.777 | 1045.09× | validated |
| filter_eq | mongo | 892.604 | 1380.42× | validated |
| filter_eq | pandas | 274.312 | 424.23× | validated |
| filter_eq | lin | 0.647 | 1.00× | validated |
| filter_eq | sqlite | 67.707 | 104.71× | validated |
| text_substr | duckdb | 102.341 | 6.45× | validated |
| text_substr | postgres | 788.451 | 49.67× | validated |
| text_substr | mysql | 1277.826 | 80.51× | validated |
| text_substr | mongo | 3209.146 | 202.19× | validated |
| text_substr | pandas | 468.608 | 29.52× | validated |
| text_substr | lin | 15.872 | 1.00× | validated |
| text_substr | sqlite | 314.131 | 19.79× | validated |
| materialize | duckdb | 1266.945 | 3.52× | validated |
| materialize | postgres | 1680.465 | 4.67× | validated |
| materialize | mysql | 9793.583 | 27.19× | validated |
| materialize | mongo | 5061.854 | 14.05× | validated |
| materialize | pandas | 1801.625 | 5.00× | validated |
| materialize | lin | 360.165 | 1.00× | validated |
| materialize | sqlite | 1873.052 | 5.20× | validated |
| join_inner | duckdb | 2674.084 | 2.15× | validated |
| join_inner | postgres | 4006.229 | 3.22× | validated |
| join_inner | mysql | 21034.145 | 16.89× | validated |
| join_inner | mongo | 114920.375 | 92.27× | validated |
| join_inner | pandas | 3845.979 | 3.09× | validated |
| join_inner | lin | 1245.521 | 1.00× | validated |
| join_inner | sqlite | 4450.292 | 3.57× | validated |
| join_filter | duckdb | 1460.007 | 2.32× | validated |
| join_filter | postgres | 2333.604 | 3.71× | validated |
| join_filter | mysql | 10762.396 | 17.12× | validated |
| join_filter | mongo | 62663.854 | 99.68× | validated |
| join_filter | pandas | 2193.635 | 3.49× | validated |
| join_filter | lin | 628.676 | 1.00× | validated |
| join_filter | sqlite | 2254.521 | 3.59× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
