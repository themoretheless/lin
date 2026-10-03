# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.955 | 3.75× | validated |
| point_get | duckdb | 39.972 | 156.83× | validated |
| point_get | postgres | 265.194 | 1040.51× | validated |
| point_get | mysql | 309.344 | 1213.74× | validated |
| point_get | mongo | 482.825 | 1894.41× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.569 | 17.93× | validated |
| point_get | lin | 0.255 | 1.00× | validated |
| filter_eq | sqlite | 69.685 | 88.56× | validated |
| filter_eq | duckdb | 181.979 | 231.26× | validated |
| filter_eq | postgres | 492.956 | 626.46× | validated |
| filter_eq | mysql | 655.090 | 832.50× | validated |
| filter_eq | mongo | 810.750 | 1030.31× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 255.448 | 324.63× | validated |
| filter_eq | lin | 0.787 | 1.00× | validated |
| text_substr | sqlite | 341.787 | 18.17× | validated |
| text_substr | duckdb | 95.287 | 5.06× | validated |
| text_substr | postgres | 765.830 | 40.70× | validated |
| text_substr | mysql | 1275.174 | 67.77× | validated |
| text_substr | mongo | 2851.562 | 151.56× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 492.961 | 26.20× | validated |
| text_substr | lin | 18.815 | 1.00× | validated |
| materialize | sqlite | 1908.386 | 4.23× | validated |
| materialize | duckdb | 1243.055 | 2.75× | validated |
| materialize | postgres | 1620.195 | 3.59× | validated |
| materialize | mysql | 10036.646 | 22.23× | validated |
| materialize | mongo | 5355.125 | 11.86× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 1824.614 | 4.04× | validated |
| materialize | lin | 451.396 | 1.00× | validated |
| join_inner | sqlite | 4556.480 | 3.14× | validated |
| join_inner | duckdb | 2594.396 | 1.79× | validated |
| join_inner | postgres | 3947.146 | 2.72× | validated |
| join_inner | mysql | 21539.188 | 14.86× | validated |
| join_inner | mongo | 109530.625 | 75.56× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 3976.187 | 2.74× | validated |
| join_inner | lin | 1449.528 | 1.00× | validated |
| join_filter | sqlite | 2327.042 | 3.19× | validated |
| join_filter | duckdb | 1401.764 | 1.92× | validated |
| join_filter | postgres | 2270.271 | 3.11× | validated |
| join_filter | mysql | 11149.229 | 15.29× | validated |
| join_filter | mongo | 54645.395 | 74.92× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 2325.938 | 3.19× | validated |
| join_filter | lin | 729.389 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
