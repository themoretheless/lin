# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.246 | 1.00× | validated |
| point_get | sqlite | 1.026 | 4.17× | validated |
| point_get | duckdb | 35.663 | 145.04× | validated |
| point_get | pandas | 4.501 | 18.30× | validated |
| filter_eq | lin | 0.774 | 1.00× | validated |
| filter_eq | sqlite | 715.271 | 923.78× | validated |
| filter_eq | duckdb | 450.571 | 581.92× | validated |
| filter_eq | pandas | 2300.104 | 2970.62× | validated |
| text_substr | lin | 238.993 | 1.00× | validated |
| text_substr | sqlite | 3524.042 | 14.75× | validated |
| text_substr | duckdb | 477.338 | 2.00× | validated |
| text_substr | pandas | 4958.875 | 20.75× | validated |
| materialize | lin | 5787.125 | 1.00× | validated |
| materialize | sqlite | 21854.958 | 3.78× | validated |
| materialize | duckdb | 12496.792 | 2.16× | validated |
| materialize | pandas | 18140.125 | 3.13× | validated |
| join_inner | lin | 14291.750 | 1.00× | validated |
| join_inner | sqlite | 53254.458 | 3.73× | validated |
| join_inner | duckdb | 29549.834 | 2.07× | validated |
| join_inner | pandas | 36483.833 | 2.55× | validated |
| join_filter | lin | 7117.292 | 1.00× | validated |
| join_filter | sqlite | 26310.042 | 3.70× | validated |
| join_filter | duckdb | 14555.958 | 2.05× | validated |
| join_filter | pandas | 19410.834 | 2.73× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
