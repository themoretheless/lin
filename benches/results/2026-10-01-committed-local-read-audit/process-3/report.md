# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 34.964 | 142.20× | validated |
| point_get | pandas | 4.519 | 18.38× | validated |
| point_get | lin | 0.246 | 1.00× | validated |
| point_get | sqlite | 1.026 | 4.17× | validated |
| filter_eq | duckdb | 440.462 | 565.71× | validated |
| filter_eq | pandas | 2278.146 | 2925.97× | validated |
| filter_eq | lin | 0.779 | 1.00× | validated |
| filter_eq | sqlite | 709.521 | 911.28× | validated |
| text_substr | duckdb | 476.967 | 2.05× | validated |
| text_substr | pandas | 5213.041 | 22.44× | validated |
| text_substr | lin | 232.300 | 1.00× | validated |
| text_substr | sqlite | 3483.167 | 14.99× | validated |
| materialize | duckdb | 12985.958 | 2.24× | validated |
| materialize | pandas | 18358.125 | 3.17× | validated |
| materialize | lin | 5787.125 | 1.00× | validated |
| materialize | sqlite | 21854.958 | 3.78× | validated |
| join_inner | duckdb | 29528.583 | 2.10× | validated |
| join_inner | pandas | 53917.833 | 3.83× | validated |
| join_inner | lin | 14059.625 | 1.00× | validated |
| join_inner | sqlite | 71023.250 | 5.05× | validated |
| join_filter | duckdb | 14924.625 | 2.00× | validated |
| join_filter | pandas | 19410.834 | 2.60× | validated |
| join_filter | lin | 7463.458 | 1.00× | validated |
| join_filter | sqlite | 26642.708 | 3.57× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
