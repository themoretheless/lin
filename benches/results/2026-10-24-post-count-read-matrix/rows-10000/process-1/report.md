# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.115 | 1.00× | validated |
| point_get | sqlite | 0.891 | 7.72× | validated |
| point_get | duckdb | 39.967 | 346.48× | validated |
| point_get | pandas | 4.606 | 39.93× | validated |
| filter_eq | lin | 0.652 | 1.00× | validated |
| filter_eq | sqlite | 67.678 | 103.77× | validated |
| filter_eq | duckdb | 187.902 | 288.11× | validated |
| filter_eq | pandas | 264.250 | 405.18× | validated |
| text_substr | lin | 16.575 | 1.00× | validated |
| text_substr | sqlite | 326.297 | 19.69× | validated |
| text_substr | duckdb | 103.488 | 6.24× | validated |
| text_substr | pandas | 480.370 | 28.98× | validated |
| materialize | lin | 396.522 | 1.00× | validated |
| materialize | sqlite | 1907.646 | 4.81× | validated |
| materialize | duckdb | 1287.445 | 3.25× | validated |
| materialize | pandas | 1816.667 | 4.58× | validated |
| join_inner | lin | 1269.736 | 1.00× | validated |
| join_inner | sqlite | 4524.458 | 3.56× | validated |
| join_inner | duckdb | 2796.562 | 2.20× | validated |
| join_inner | pandas | 3846.479 | 3.03× | validated |
| join_filter | lin | 680.205 | 1.00× | validated |
| join_filter | sqlite | 2270.760 | 3.34× | validated |
| join_filter | duckdb | 1546.583 | 2.27× | validated |
| join_filter | pandas | 2229.771 | 3.28× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
