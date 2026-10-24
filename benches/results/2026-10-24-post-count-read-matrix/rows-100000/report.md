# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.116 | 1.00× | validated |
| point_get | sqlite | 0.982 | 8.50× | validated |
| point_get | duckdb | 39.810 | 344.58× | validated |
| point_get | pandas | 4.395 | 38.04× | validated |
| filter_eq | lin | 0.844 | 1.00× | validated |
| filter_eq | sqlite | 706.926 | 837.73× | validated |
| filter_eq | duckdb | 439.148 | 520.40× | validated |
| filter_eq | pandas | 2194.708 | 2600.79× | validated |
| text_substr | lin | 206.620 | 1.00× | validated |
| text_substr | sqlite | 3444.562 | 16.67× | validated |
| text_substr | duckdb | 475.194 | 2.30× | validated |
| text_substr | pandas | 4473.292 | 21.65× | validated |
| materialize | lin | 4705.646 | 1.00× | validated |
| materialize | sqlite | 21204.667 | 4.51× | validated |
| materialize | duckdb | 13032.021 | 2.77× | validated |
| materialize | pandas | 18369.958 | 3.90× | validated |
| join_inner | lin | 15773.334 | 1.00× | validated |
| join_inner | sqlite | 54282.375 | 3.44× | validated |
| join_inner | duckdb | 29638.896 | 1.88× | validated |
| join_inner | pandas | 35529.395 | 2.25× | validated |
| join_filter | lin | 7951.562 | 1.00× | validated |
| join_filter | sqlite | 26839.688 | 3.38× | validated |
| join_filter | duckdb | 14510.334 | 1.82× | validated |
| join_filter | pandas | 19033.708 | 2.39× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
