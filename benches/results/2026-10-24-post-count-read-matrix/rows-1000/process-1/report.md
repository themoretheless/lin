# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.112 | 1.00× | validated |
| point_get | sqlite | 0.893 | 7.98× | validated |
| point_get | duckdb | 39.379 | 351.86× | validated |
| point_get | pandas | 4.419 | 39.48× | validated |
| filter_eq | lin | 0.632 | 1.00× | validated |
| filter_eq | sqlite | 7.364 | 11.66× | validated |
| filter_eq | duckdb | 129.157 | 204.47× | validated |
| filter_eq | pandas | 54.833 | 86.81× | validated |
| text_substr | lin | 1.750 | 1.00× | validated |
| text_substr | sqlite | 31.051 | 17.75× | validated |
| text_substr | duckdb | 57.717 | 32.99× | validated |
| text_substr | pandas | 73.047 | 41.75× | validated |
| materialize | lin | 43.053 | 1.00× | validated |
| materialize | sqlite | 164.253 | 3.82× | validated |
| materialize | duckdb | 295.939 | 6.87× | validated |
| materialize | pandas | 368.434 | 8.56× | validated |
| join_inner | lin | 136.456 | 1.00× | validated |
| join_inner | sqlite | 389.283 | 2.85× | validated |
| join_inner | duckdb | 349.414 | 2.56× | validated |
| join_inner | pandas | 721.017 | 5.28× | validated |
| join_filter | lin | 69.933 | 1.00× | validated |
| join_filter | sqlite | 197.245 | 2.82× | validated |
| join_filter | duckdb | 240.522 | 3.44× | validated |
| join_filter | pandas | 623.045 | 8.91× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
