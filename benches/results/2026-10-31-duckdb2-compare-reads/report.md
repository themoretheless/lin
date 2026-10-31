# Validated read API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.107 | 1.00× | validated |
| point_get | sqlite | 0.956 | 8.92× | validated |
| point_get | duckdb | 111.265 | 1038.22× | validated |
| filter_eq | lin | 0.572 | 1.00× | validated |
| filter_eq | sqlite | 69.628 | 121.82× | validated |
| filter_eq | duckdb | 263.587 | 461.16× | validated |
| text_substr | lin | 17.410 | 1.00× | validated |
| text_substr | sqlite | 325.867 | 18.72× | validated |
| text_substr | duckdb | 230.988 | 13.27× | validated |
| materialize | lin | 369.442 | 1.00× | validated |
| materialize | sqlite | 1895.698 | 5.13× | validated |
| materialize | duckdb | 1543.417 | 4.18× | validated |
| join_inner | lin | 1346.583 | 1.00× | validated |
| join_inner | sqlite | 4583.917 | 3.40× | validated |
| join_inner | duckdb | 3212.521 | 2.39× | validated |
| join_filter | lin | 641.383 | 1.00× | validated |
| join_filter | sqlite | 2320.500 | 3.62× | validated |
| join_filter | duckdb | 1904.146 | 2.97× | validated |

Versions: {"duckdb": "2.0.0", "lin": "0.4.0", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
