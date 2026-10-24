# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 40.114 | 336.03× | validated |
| point_get | postgres | 178.298 | 1493.57× | validated |
| point_get | mysql | 332.786 | 2787.69× | validated |
| point_get | mongo | 348.639 | 2920.48× | validated |
| point_get | pandas | 4.401 | 36.87× | validated |
| point_get | lin | 0.119 | 1.00× | validated |
| point_get | sqlite | 0.935 | 7.83× | validated |
| filter_eq | duckdb | 181.877 | 258.91× | validated |
| filter_eq | postgres | 430.176 | 612.38× | validated |
| filter_eq | mysql | 620.054 | 882.68× | validated |
| filter_eq | mongo | 864.326 | 1230.42× | validated |
| filter_eq | pandas | 255.935 | 364.34× | validated |
| filter_eq | lin | 0.702 | 1.00× | validated |
| filter_eq | sqlite | 69.449 | 98.86× | validated |
| text_substr | duckdb | 94.838 | 5.92× | validated |
| text_substr | postgres | 718.101 | 44.79× | validated |
| text_substr | mysql | 1247.844 | 77.83× | validated |
| text_substr | mongo | 3012.479 | 187.90× | validated |
| text_substr | pandas | 472.223 | 29.45× | validated |
| text_substr | lin | 16.032 | 1.00× | validated |
| text_substr | sqlite | 341.186 | 21.28× | validated |
| materialize | duckdb | 1221.156 | 3.33× | validated |
| materialize | postgres | 1738.073 | 4.74× | validated |
| materialize | mysql | 9478.062 | 25.86× | validated |
| materialize | mongo | 4888.625 | 13.34× | validated |
| materialize | pandas | 1818.417 | 4.96× | validated |
| materialize | lin | 366.471 | 1.00× | validated |
| materialize | sqlite | 1865.729 | 5.09× | validated |
| join_inner | duckdb | 2633.375 | 1.91× | validated |
| join_inner | postgres | 3576.667 | 2.59× | validated |
| join_inner | mysql | 20354.917 | 14.76× | validated |
| join_inner | mongo | 112877.000 | 81.87× | validated |
| join_inner | pandas | 3841.875 | 2.79× | validated |
| join_inner | lin | 1378.778 | 1.00× | validated |
| join_inner | sqlite | 4540.188 | 3.29× | validated |
| join_filter | duckdb | 1389.146 | 2.10× | validated |
| join_filter | postgres | 2085.468 | 3.15× | validated |
| join_filter | mysql | 10688.479 | 16.15× | validated |
| join_filter | mongo | 55982.542 | 84.58× | validated |
| join_filter | pandas | 2214.677 | 3.35× | validated |
| join_filter | lin | 661.893 | 1.00× | validated |
| join_filter | sqlite | 2268.615 | 3.43× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
