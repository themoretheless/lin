# Validated read API benchmark

Rows: 1000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.174 | 1.00× | validated |
| point_get | sqlite | 0.934 | 5.37× | validated |
| point_get | duckdb | 41.990 | 241.23× | validated |
| point_get | pandas | 4.581 | 26.32× | validated |
| filter_eq | lin | 58.269 | 1.00× | validated |
| filter_eq | sqlite | 7.569 | 0.13× | validated |
| filter_eq | duckdb | 147.864 | 2.54× | validated |
| filter_eq | pandas | 161.707 | 2.78× | validated |
| text_substr | lin | 43.772 | 1.00× | validated |
| text_substr | sqlite | 32.073 | 0.73× | validated |
| text_substr | duckdb | 74.813 | 1.71× | validated |
| text_substr | pandas | 98.630 | 2.25× | validated |
| materialize | lin | 39.214 | 1.00× | validated |
| materialize | sqlite | 165.371 | 4.22× | validated |
| materialize | duckdb | 303.207 | 7.73× | validated |
| materialize | pandas | 475.229 | 12.12× | validated |
| join_inner | lin | 133.981 | 1.00× | validated |
| join_inner | sqlite | 393.766 | 2.94× | validated |
| join_inner | duckdb | 406.951 | 3.04× | validated |
| join_inner | pandas | 1504.042 | 11.23× | validated |
| join_filter | lin | 67.269 | 1.00× | validated |
| join_filter | sqlite | 203.813 | 3.03× | validated |
| join_filter | duckdb | 268.233 | 3.99× | validated |
| join_filter | pandas | 6962.230 | 103.50× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
