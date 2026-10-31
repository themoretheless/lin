# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.106 | 1.00× | validated |
| point_get | sqlite | 0.949 | 8.95× | validated |
| point_get | duckdb | 108.890 | 1026.78× | validated |
| filter_eq | lin | 0.572 | 1.00× | validated |
| filter_eq | sqlite | 69.628 | 121.82× | validated |
| filter_eq | duckdb | 263.587 | 461.16× | validated |
| text_substr | lin | 17.182 | 1.00× | validated |
| text_substr | sqlite | 335.830 | 19.54× | validated |
| text_substr | duckdb | 230.988 | 13.44× | validated |
| materialize | lin | 367.717 | 1.00× | validated |
| materialize | sqlite | 1919.354 | 5.22× | validated |
| materialize | duckdb | 1588.062 | 4.32× | validated |
| join_inner | lin | 1346.583 | 1.00× | validated |
| join_inner | sqlite | 4583.917 | 3.40× | validated |
| join_inner | duckdb | 3168.979 | 2.35× | validated |
| join_filter | lin | 641.383 | 1.00× | validated |
| join_filter | sqlite | 2319.000 | 3.62× | validated |
| join_filter | duckdb | 1904.146 | 2.97× | validated |

Versions: {"duckdb": "2.0.0", "lin": "0.4.0", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
