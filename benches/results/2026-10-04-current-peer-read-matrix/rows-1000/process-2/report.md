# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 1.311 | 2.13× | validated |
| point_get | duckdb | 50.659 | 82.37× | validated |
| point_get | postgres | 404.178 | 657.18× | validated |
| point_get | mysql | 997.583 | 1622.03× | validated |
| point_get | mongo | 645.029 | 1048.79× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.372 | 10.36× | validated |
| point_get | lin | 0.615 | 1.00× | validated |
| filter_eq | sqlite | 10.678 | 6.74× | validated |
| filter_eq | duckdb | 186.751 | 117.83× | validated |
| filter_eq | postgres | 436.203 | 275.23× | validated |
| filter_eq | mysql | 815.075 | 514.28× | validated |
| filter_eq | mongo | 685.619 | 432.60× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 93.113 | 58.75× | validated |
| filter_eq | lin | 1.585 | 1.00× | validated |
| text_substr | sqlite | 44.849 | 10.62× | validated |
| text_substr | duckdb | 84.870 | 20.09× | validated |
| text_substr | postgres | 466.361 | 110.38× | validated |
| text_substr | mysql | 881.046 | 208.54× | validated |
| text_substr | mongo | 904.604 | 214.11× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 138.622 | 32.81× | validated |
| text_substr | lin | 4.225 | 1.00× | validated |
| materialize | sqlite | 237.482 | 2.52× | validated |
| materialize | duckdb | 401.016 | 4.26× | validated |
| materialize | postgres | 594.824 | 6.32× | validated |
| materialize | mysql | 2844.875 | 30.22× | validated |
| materialize | mongo | 1679.511 | 17.84× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 709.057 | 7.53× | validated |
| materialize | lin | 94.135 | 1.00× | validated |
| join_inner | sqlite | 550.146 | 2.24× | validated |
| join_inner | duckdb | 484.254 | 1.97× | validated |
| join_inner | postgres | 1339.271 | 5.46× | validated |
| join_inner | mysql | 5524.021 | 22.50× | validated |
| join_inner | mongo | 19176.020 | 78.11× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 1536.695 | 6.26× | validated |
| join_inner | lin | 245.506 | 1.00× | validated |
| join_filter | sqlite | 282.299 | 2.30× | validated |
| join_filter | duckdb | 330.324 | 2.69× | validated |
| join_filter | postgres | 1236.930 | 10.06× | validated |
| join_filter | mysql | 3918.188 | 31.87× | validated |
| join_filter | mongo | 10580.938 | 86.07× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 1316.375 | 10.71× | validated |
| join_filter | lin | 122.940 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
