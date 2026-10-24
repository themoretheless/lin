# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.944 | 5.14× | validated |
| point_get | duckdb | 34.929 | 190.02× | validated |
| point_get | pandas | 4.380 | 23.83× | validated |
| point_get | lin | 0.184 | 1.00× | validated |
| filter_eq | sqlite | 7.683 | 0.08× | validated |
| filter_eq | duckdb | 124.500 | 1.30× | validated |
| filter_eq | pandas | 365.786 | 3.81× | validated |
| filter_eq | lin | 95.901 | 1.00× | validated |
| text_substr | sqlite | 31.656 | 0.47× | validated |
| text_substr | duckdb | 54.468 | 0.81× | validated |
| text_substr | pandas | 98.630 | 1.46× | validated |
| text_substr | lin | 67.605 | 1.00× | validated |
| materialize | sqlite | 163.584 | 2.11× | validated |
| materialize | duckdb | 292.651 | 3.77× | validated |
| materialize | pandas | 475.229 | 6.12× | validated |
| materialize | lin | 77.613 | 1.00× | validated |
| join_inner | sqlite | 393.766 | 1.39× | validated |
| join_inner | duckdb | 363.213 | 1.28× | validated |
| join_inner | pandas | 2567.167 | 9.08× | validated |
| join_inner | lin | 282.720 | 1.00× | validated |
| join_filter | sqlite | 200.771 | 0.48× | validated |
| join_filter | duckdb | 246.317 | 0.59× | validated |
| join_filter | pandas | 7095.432 | 17.00× | validated |
| join_filter | lin | 417.476 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
