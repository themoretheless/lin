# Validated read API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.247 | 1.00× | validated |
| point_get | sqlite | 0.965 | 3.90× | validated |
| point_get | duckdb | 39.972 | 161.77× | validated |
| point_get | postgres | 271.038 | 1096.89× | validated |
| point_get | mysql | 309.344 | 1251.92× | validated |
| point_get | mongo | 482.825 | 1954.00× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.569 | 18.49× | validated |
| filter_eq | lin | 0.684 | 1.00× | validated |
| filter_eq | sqlite | 69.685 | 101.86× | validated |
| filter_eq | duckdb | 181.979 | 265.99× | validated |
| filter_eq | postgres | 492.956 | 720.54× | validated |
| filter_eq | mysql | 655.090 | 957.52× | validated |
| filter_eq | mongo | 810.750 | 1185.04× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 262.681 | 383.95× | validated |
| text_substr | lin | 17.382 | 1.00× | validated |
| text_substr | sqlite | 334.554 | 19.25× | validated |
| text_substr | duckdb | 98.647 | 5.68× | validated |
| text_substr | postgres | 778.479 | 44.79× | validated |
| text_substr | mysql | 1266.174 | 72.84× | validated |
| text_substr | mongo | 2880.521 | 165.72× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 503.404 | 28.96× | validated |
| materialize | lin | 421.597 | 1.00× | validated |
| materialize | sqlite | 1946.333 | 4.62× | validated |
| materialize | duckdb | 1243.055 | 2.95× | validated |
| materialize | postgres | 1683.458 | 3.99× | validated |
| materialize | mysql | 10036.646 | 23.81× | validated |
| materialize | mongo | 5355.125 | 12.70× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 1824.614 | 4.33× | validated |
| join_inner | lin | 1357.840 | 1.00× | validated |
| join_inner | sqlite | 4556.480 | 3.36× | validated |
| join_inner | duckdb | 2679.604 | 1.97× | validated |
| join_inner | postgres | 3860.562 | 2.84× | validated |
| join_inner | mysql | 21539.188 | 15.86× | validated |
| join_inner | mongo | 113076.687 | 83.28× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 3976.187 | 2.93× | validated |
| join_filter | lin | 663.809 | 1.00× | validated |
| join_filter | sqlite | 2344.354 | 3.53× | validated |
| join_filter | duckdb | 1423.611 | 2.14× | validated |
| join_filter | postgres | 2214.427 | 3.34× | validated |
| join_filter | mysql | 11149.229 | 16.80× | validated |
| join_filter | mongo | 56145.438 | 84.58× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 2332.010 | 3.51× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
