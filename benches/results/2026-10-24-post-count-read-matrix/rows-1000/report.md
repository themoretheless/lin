# Validated read API benchmark

Rows: 1000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.113 | 1.00× | validated |
| point_get | sqlite | 0.893 | 7.89× | validated |
| point_get | duckdb | 39.905 | 352.34× | validated |
| point_get | pandas | 4.419 | 39.02× | validated |
| filter_eq | lin | 0.632 | 1.00× | validated |
| filter_eq | sqlite | 7.381 | 11.68× | validated |
| filter_eq | duckdb | 130.319 | 206.31× | validated |
| filter_eq | pandas | 55.775 | 88.30× | validated |
| text_substr | lin | 1.784 | 1.00× | validated |
| text_substr | sqlite | 31.051 | 17.40× | validated |
| text_substr | duckdb | 57.717 | 32.35× | validated |
| text_substr | pandas | 73.344 | 41.10× | validated |
| materialize | lin | 36.849 | 1.00× | validated |
| materialize | sqlite | 164.253 | 4.46× | validated |
| materialize | duckdb | 295.914 | 8.03× | validated |
| materialize | pandas | 364.792 | 9.90× | validated |
| join_inner | lin | 134.142 | 1.00× | validated |
| join_inner | sqlite | 389.283 | 2.90× | validated |
| join_inner | duckdb | 345.870 | 2.58× | validated |
| join_inner | pandas | 702.681 | 5.24× | validated |
| join_filter | lin | 69.933 | 1.00× | validated |
| join_filter | sqlite | 202.139 | 2.89× | validated |
| join_filter | duckdb | 241.596 | 3.45× | validated |
| join_filter | pandas | 615.888 | 8.81× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
