# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.243 | 1.00× | validated |
| point_get | sqlite | 1.026 | 4.23× | validated |
| point_get | duckdb | 35.972 | 148.33× | validated |
| point_get | pandas | 4.501 | 18.56× | validated |
| filter_eq | lin | 0.770 | 1.00× | validated |
| filter_eq | sqlite | 717.024 | 930.87× | validated |
| filter_eq | duckdb | 450.571 | 584.95× | validated |
| filter_eq | pandas | 2308.729 | 2997.30× | validated |
| text_substr | lin | 238.993 | 1.00× | validated |
| text_substr | sqlite | 3564.250 | 14.91× | validated |
| text_substr | duckdb | 477.338 | 2.00× | validated |
| text_substr | pandas | 4895.750 | 20.48× | validated |
| materialize | lin | 5468.542 | 1.00× | validated |
| materialize | sqlite | 22069.709 | 4.04× | validated |
| materialize | duckdb | 12186.458 | 2.23× | validated |
| materialize | pandas | 18140.125 | 3.32× | validated |
| join_inner | lin | 14411.625 | 1.00× | validated |
| join_inner | sqlite | 53254.458 | 3.70× | validated |
| join_inner | duckdb | 29549.834 | 2.05× | validated |
| join_inner | pandas | 36483.833 | 2.53× | validated |
| join_filter | lin | 7060.083 | 1.00× | validated |
| join_filter | sqlite | 26310.042 | 3.73× | validated |
| join_filter | duckdb | 14555.958 | 2.06× | validated |
| join_filter | pandas | 19290.750 | 2.73× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
