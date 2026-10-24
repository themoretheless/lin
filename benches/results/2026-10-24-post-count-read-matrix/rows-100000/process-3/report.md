# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 39.810 | 364.35× | validated |
| point_get | pandas | 4.395 | 40.22× | validated |
| point_get | lin | 0.109 | 1.00× | validated |
| point_get | sqlite | 0.960 | 8.79× | validated |
| filter_eq | duckdb | 408.491 | 491.21× | validated |
| filter_eq | pandas | 2152.740 | 2588.67× | validated |
| filter_eq | lin | 0.832 | 1.00× | validated |
| filter_eq | sqlite | 683.482 | 821.89× | validated |
| text_substr | duckdb | 475.194 | 2.31× | validated |
| text_substr | pandas | 4449.750 | 21.64× | validated |
| text_substr | lin | 205.643 | 1.00× | validated |
| text_substr | sqlite | 3242.251 | 15.77× | validated |
| materialize | duckdb | 12618.709 | 2.68× | validated |
| materialize | pandas | 18369.958 | 3.90× | validated |
| materialize | lin | 4705.646 | 1.00× | validated |
| materialize | sqlite | 20101.667 | 4.27× | validated |
| join_inner | duckdb | 28546.229 | 1.81× | validated |
| join_inner | pandas | 35529.395 | 2.26× | validated |
| join_inner | lin | 15749.021 | 1.00× | validated |
| join_inner | sqlite | 50919.458 | 3.23× | validated |
| join_filter | duckdb | 14482.416 | 1.82× | validated |
| join_filter | pandas | 19033.708 | 2.39× | validated |
| join_filter | lin | 7951.562 | 1.00× | validated |
| join_filter | sqlite | 24972.583 | 3.14× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
