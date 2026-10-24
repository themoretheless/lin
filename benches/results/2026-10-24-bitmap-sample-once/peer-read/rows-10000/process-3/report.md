# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 40.720 | 346.18× | validated |
| point_get | postgres | 249.559 | 2121.65× | validated |
| point_get | mysql | 335.130 | 2849.14× | validated |
| point_get | mongo | 415.406 | 3531.62× | validated |
| point_get | pandas | 4.428 | 37.65× | validated |
| point_get | lin | 0.118 | 1.00× | validated |
| point_get | sqlite | 0.977 | 8.31× | validated |
| filter_eq | duckdb | 184.786 | 256.18× | validated |
| filter_eq | postgres | 444.036 | 615.60× | validated |
| filter_eq | mysql | 635.214 | 880.64× | validated |
| filter_eq | mongo | 879.646 | 1219.52× | validated |
| filter_eq | pandas | 253.824 | 351.89× | validated |
| filter_eq | lin | 0.721 | 1.00× | validated |
| filter_eq | sqlite | 68.477 | 94.93× | validated |
| text_substr | duckdb | 95.678 | 5.92× | validated |
| text_substr | postgres | 750.073 | 46.38× | validated |
| text_substr | mysql | 1276.167 | 78.91× | validated |
| text_substr | mongo | 2967.229 | 183.47× | validated |
| text_substr | pandas | 473.423 | 29.27× | validated |
| text_substr | lin | 16.173 | 1.00× | validated |
| text_substr | sqlite | 339.397 | 20.99× | validated |
| materialize | duckdb | 1217.385 | 3.26× | validated |
| materialize | postgres | 1852.844 | 4.96× | validated |
| materialize | mysql | 9845.083 | 26.34× | validated |
| materialize | mongo | 5223.605 | 13.98× | validated |
| materialize | pandas | 1895.281 | 5.07× | validated |
| materialize | lin | 373.729 | 1.00× | validated |
| materialize | sqlite | 1909.531 | 5.11× | validated |
| join_inner | duckdb | 2633.188 | 1.97× | validated |
| join_inner | postgres | 4359.583 | 3.26× | validated |
| join_inner | mysql | 20872.709 | 15.60× | validated |
| join_inner | mongo | 115540.458 | 86.34× | validated |
| join_inner | pandas | 4239.896 | 3.17× | validated |
| join_inner | lin | 1338.271 | 1.00× | validated |
| join_inner | sqlite | 4591.500 | 3.43× | validated |
| join_filter | duckdb | 1390.215 | 2.08× | validated |
| join_filter | postgres | 2163.927 | 3.24× | validated |
| join_filter | mysql | 11231.479 | 16.84× | validated |
| join_filter | mongo | 61045.375 | 91.53× | validated |
| join_filter | pandas | 2278.396 | 3.42× | validated |
| join_filter | lin | 666.967 | 1.00× | validated |
| join_filter | sqlite | 2384.760 | 3.58× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
