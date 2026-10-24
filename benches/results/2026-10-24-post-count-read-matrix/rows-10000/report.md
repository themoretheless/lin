# Validated read API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.115 | 1.00× | validated |
| point_get | sqlite | 0.893 | 7.74× | validated |
| point_get | duckdb | 40.129 | 347.89× | validated |
| point_get | pandas | 4.392 | 38.07× | validated |
| filter_eq | lin | 0.664 | 1.00× | validated |
| filter_eq | sqlite | 67.678 | 101.86× | validated |
| filter_eq | duckdb | 185.892 | 279.79× | validated |
| filter_eq | pandas | 264.250 | 397.73× | validated |
| text_substr | lin | 16.654 | 1.00× | validated |
| text_substr | sqlite | 326.297 | 19.59× | validated |
| text_substr | duckdb | 103.488 | 6.21× | validated |
| text_substr | pandas | 480.370 | 28.84× | validated |
| materialize | lin | 396.226 | 1.00× | validated |
| materialize | sqlite | 1907.646 | 4.81× | validated |
| materialize | duckdb | 1287.445 | 3.25× | validated |
| materialize | pandas | 1816.667 | 4.58× | validated |
| join_inner | lin | 1364.403 | 1.00× | validated |
| join_inner | sqlite | 4524.458 | 3.32× | validated |
| join_inner | duckdb | 2617.459 | 1.92× | validated |
| join_inner | pandas | 3846.479 | 2.82× | validated |
| join_filter | lin | 691.426 | 1.00× | validated |
| join_filter | sqlite | 2334.146 | 3.38× | validated |
| join_filter | duckdb | 1440.708 | 2.08× | validated |
| join_filter | pandas | 2208.135 | 3.19× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
