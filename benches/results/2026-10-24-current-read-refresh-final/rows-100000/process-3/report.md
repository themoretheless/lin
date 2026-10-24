# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 39.578 | 326.00× | validated |
| point_get | postgres | 232.064 | 1911.50× | validated |
| point_get | mysql | 325.083 | 2677.70× | validated |
| point_get | mongo | 506.033 | 4168.17× | validated |
| point_get | pandas | 4.411 | 36.33× | validated |
| point_get | lin | 0.121 | 1.00× | validated |
| point_get | sqlite | 0.978 | 8.06× | validated |
| filter_eq | duckdb | 407.398 | 482.40× | validated |
| filter_eq | postgres | 2209.188 | 2615.88× | validated |
| filter_eq | mysql | 3326.604 | 3939.00× | validated |
| filter_eq | mongo | 5061.125 | 5992.83× | validated |
| filter_eq | pandas | 2181.146 | 2582.67× | validated |
| filter_eq | lin | 0.845 | 1.00× | validated |
| filter_eq | sqlite | 670.634 | 794.09× | validated |
| text_substr | duckdb | 451.560 | 2.18× | validated |
| text_substr | postgres | 4915.480 | 23.69× | validated |
| text_substr | mysql | 9324.749 | 44.95× | validated |
| text_substr | mongo | 27405.896 | 132.10× | validated |
| text_substr | pandas | 4485.980 | 21.62× | validated |
| text_substr | lin | 207.469 | 1.00× | validated |
| text_substr | sqlite | 3234.229 | 15.59× | validated |
| materialize | duckdb | 12163.146 | 2.13× | validated |
| materialize | postgres | 13751.312 | 2.41× | validated |
| materialize | mysql | 83532.083 | 14.64× | validated |
| materialize | mongo | 42899.459 | 7.52× | validated |
| materialize | pandas | 17108.730 | 3.00× | validated |
| materialize | lin | 5703.874 | 1.00× | validated |
| materialize | sqlite | 20086.438 | 3.52× | validated |
| join_inner | duckdb | 28329.709 | 1.80× | validated |
| join_inner | postgres | 38994.417 | 2.47× | validated |
| join_inner | mysql | 168532.958 | 10.69× | validated |
| join_inner | mongo | 1116295.646 | 70.83× | validated |
| join_inner | pandas | 35976.834 | 2.28× | validated |
| join_inner | lin | 15761.209 | 1.00× | validated |
| join_inner | sqlite | 49717.334 | 3.15× | validated |
| join_filter | duckdb | 13654.771 | 1.93× | validated |
| join_filter | postgres | 21015.146 | 2.98× | validated |
| join_filter | mysql | 91354.063 | 12.94× | validated |
| join_filter | mongo | 573675.917 | 81.25× | validated |
| join_filter | pandas | 18761.834 | 2.66× | validated |
| join_filter | lin | 7060.708 | 1.00× | validated |
| join_filter | sqlite | 25587.438 | 3.62× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
