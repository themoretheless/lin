# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 37.406 | 322.73× | validated |
| point_get | pandas | 4.337 | 37.42× | validated |
| point_get | lin | 0.116 | 1.00× | validated |
| point_get | sqlite | 0.997 | 8.60× | validated |
| filter_eq | duckdb | 190.983 | 0.73× | validated |
| filter_eq | pandas | 260.363 | 1.00× | validated |
| filter_eq | lin | 260.064 | 1.00× | validated |
| filter_eq | sqlite | 69.868 | 0.27× | validated |
| text_substr | duckdb | 106.388 | 0.36× | validated |
| text_substr | pandas | 473.608 | 1.61× | validated |
| text_substr | lin | 294.321 | 1.00× | validated |
| text_substr | sqlite | 333.375 | 1.13× | validated |
| materialize | duckdb | 1259.411 | 3.41× | validated |
| materialize | pandas | 1854.063 | 5.01× | validated |
| materialize | lin | 369.803 | 1.00× | validated |
| materialize | sqlite | 1907.802 | 5.16× | validated |
| join_inner | duckdb | 2622.688 | 2.03× | validated |
| join_inner | pandas | 4028.667 | 3.12× | validated |
| join_inner | lin | 1292.486 | 1.00× | validated |
| join_inner | sqlite | 4631.688 | 3.58× | validated |
| join_filter | duckdb | 1392.951 | 2.10× | validated |
| join_filter | pandas | 2296.792 | 3.47× | validated |
| join_filter | lin | 662.798 | 1.00× | validated |
| join_filter | sqlite | 2366.688 | 3.57× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
