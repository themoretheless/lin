# Validated read API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.119 | 1.00× | validated |
| point_get | sqlite | 0.977 | 8.21× | validated |
| point_get | duckdb | 40.720 | 342.05× | validated |
| point_get | postgres | 253.163 | 2126.60× | validated |
| point_get | mysql | 330.667 | 2777.64× | validated |
| point_get | mongo | 415.406 | 3489.46× | validated |
| point_get | pandas | 4.431 | 37.23× | validated |
| filter_eq | lin | 0.691 | 1.00× | validated |
| filter_eq | sqlite | 68.977 | 99.89× | validated |
| filter_eq | duckdb | 187.298 | 271.24× | validated |
| filter_eq | postgres | 463.881 | 671.79× | validated |
| filter_eq | mysql | 635.214 | 919.91× | validated |
| filter_eq | mongo | 916.933 | 1327.90× | validated |
| filter_eq | pandas | 263.228 | 381.20× | validated |
| text_substr | lin | 16.690 | 1.00× | validated |
| text_substr | sqlite | 338.336 | 20.27× | validated |
| text_substr | duckdb | 103.063 | 6.18× | validated |
| text_substr | postgres | 761.014 | 45.60× | validated |
| text_substr | mysql | 1281.542 | 76.78× | validated |
| text_substr | mongo | 3053.209 | 182.93× | validated |
| text_substr | pandas | 473.035 | 28.34× | validated |
| materialize | lin | 373.729 | 1.00× | validated |
| materialize | sqlite | 1867.146 | 5.00× | validated |
| materialize | duckdb | 1243.875 | 3.33× | validated |
| materialize | postgres | 1642.090 | 4.39× | validated |
| materialize | mysql | 9732.771 | 26.04× | validated |
| materialize | mongo | 5223.605 | 13.98× | validated |
| materialize | pandas | 1895.281 | 5.07× | validated |
| join_inner | lin | 1338.271 | 1.00× | validated |
| join_inner | sqlite | 4476.792 | 3.35× | validated |
| join_inner | duckdb | 2689.729 | 2.01× | validated |
| join_inner | postgres | 4359.583 | 3.26× | validated |
| join_inner | mysql | 20484.438 | 15.31× | validated |
| join_inner | mongo | 115260.500 | 86.13× | validated |
| join_inner | pandas | 4239.896 | 3.17× | validated |
| join_filter | lin | 666.967 | 1.00× | validated |
| join_filter | sqlite | 2336.010 | 3.50× | validated |
| join_filter | duckdb | 1447.139 | 2.17× | validated |
| join_filter | postgres | 2228.021 | 3.34× | validated |
| join_filter | mysql | 10808.834 | 16.21× | validated |
| join_filter | mongo | 58200.438 | 87.26× | validated |
| join_filter | pandas | 2278.396 | 3.42× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
