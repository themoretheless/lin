# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 1.009 | 8.73× | validated |
| point_get | duckdb | 33.965 | 293.98× | validated |
| point_get | pandas | 4.332 | 37.49× | validated |
| point_get | lin | 0.116 | 1.00× | validated |
| filter_eq | sqlite | 706.926 | 837.73× | validated |
| filter_eq | duckdb | 439.148 | 520.40× | validated |
| filter_eq | pandas | 2194.708 | 2600.79× | validated |
| filter_eq | lin | 0.844 | 1.00× | validated |
| text_substr | sqlite | 3518.104 | 17.03× | validated |
| text_substr | duckdb | 470.527 | 2.28× | validated |
| text_substr | pandas | 4473.292 | 21.65× | validated |
| text_substr | lin | 206.620 | 1.00× | validated |
| materialize | sqlite | 21500.854 | 4.71× | validated |
| materialize | duckdb | 13032.021 | 2.86× | validated |
| materialize | pandas | 16916.688 | 3.71× | validated |
| materialize | lin | 4562.021 | 1.00× | validated |
| join_inner | sqlite | 54297.542 | 3.44× | validated |
| join_inner | duckdb | 34343.188 | 2.18× | validated |
| join_inner | pandas | 35125.604 | 2.23× | validated |
| join_inner | lin | 15773.334 | 1.00× | validated |
| join_filter | sqlite | 26839.688 | 3.54× | validated |
| join_filter | duckdb | 14510.334 | 1.91× | validated |
| join_filter | pandas | 18183.896 | 2.40× | validated |
| join_filter | lin | 7582.104 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
