# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 39.905 | 352.34× | validated |
| point_get | pandas | 4.471 | 39.48× | validated |
| point_get | lin | 0.113 | 1.00× | validated |
| point_get | sqlite | 0.893 | 7.88× | validated |
| filter_eq | duckdb | 130.319 | 195.64× | validated |
| filter_eq | pandas | 56.403 | 84.67× | validated |
| filter_eq | lin | 0.666 | 1.00× | validated |
| filter_eq | sqlite | 7.381 | 11.08× | validated |
| text_substr | duckdb | 57.556 | 32.26× | validated |
| text_substr | pandas | 74.168 | 41.57× | validated |
| text_substr | lin | 1.784 | 1.00× | validated |
| text_substr | sqlite | 30.562 | 17.13× | validated |
| materialize | duckdb | 295.914 | 8.03× | validated |
| materialize | pandas | 364.792 | 9.90× | validated |
| materialize | lin | 36.849 | 1.00× | validated |
| materialize | sqlite | 165.862 | 4.50× | validated |
| join_inner | duckdb | 341.710 | 2.55× | validated |
| join_inner | pandas | 695.892 | 5.19× | validated |
| join_inner | lin | 134.142 | 1.00× | validated |
| join_inner | sqlite | 396.990 | 2.96× | validated |
| join_filter | duckdb | 242.009 | 3.39× | validated |
| join_filter | pandas | 614.125 | 8.60× | validated |
| join_filter | lin | 71.395 | 1.00× | validated |
| join_filter | sqlite | 204.326 | 2.86× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
