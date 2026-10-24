# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.118 | 1.00× | validated |
| point_get | sqlite | 0.921 | 7.83× | validated |
| point_get | duckdb | 41.990 | 357.14× | validated |
| point_get | pandas | 4.581 | 38.96× | validated |
| filter_eq | lin | 26.198 | 1.00× | validated |
| filter_eq | sqlite | 7.540 | 0.29× | validated |
| filter_eq | duckdb | 147.864 | 5.64× | validated |
| filter_eq | pandas | 57.086 | 2.18× | validated |
| text_substr | lin | 27.567 | 1.00× | validated |
| text_substr | sqlite | 32.292 | 1.17× | validated |
| text_substr | duckdb | 74.813 | 2.71× | validated |
| text_substr | pandas | 78.702 | 2.85× | validated |
| materialize | lin | 38.634 | 1.00× | validated |
| materialize | sqlite | 171.227 | 4.43× | validated |
| materialize | duckdb | 303.207 | 7.85× | validated |
| materialize | pandas | 378.814 | 9.81× | validated |
| join_inner | lin | 133.507 | 1.00× | validated |
| join_inner | sqlite | 400.990 | 3.00× | validated |
| join_inner | duckdb | 406.951 | 3.05× | validated |
| join_inner | pandas | 722.379 | 5.41× | validated |
| join_filter | lin | 67.269 | 1.00× | validated |
| join_filter | sqlite | 203.813 | 3.03× | validated |
| join_filter | duckdb | 268.233 | 3.99× | validated |
| join_filter | pandas | 646.337 | 9.61× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
