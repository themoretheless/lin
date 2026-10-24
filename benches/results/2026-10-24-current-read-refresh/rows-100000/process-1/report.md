# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.115 | 1.00× | validated |
| point_get | sqlite | 0.999 | 8.71× | validated |
| point_get | duckdb | 40.585 | 353.90× | validated |
| point_get | pandas | 4.484 | 39.10× | validated |
| filter_eq | lin | 5836.501 | 1.00× | validated |
| filter_eq | sqlite | 692.587 | 0.12× | validated |
| filter_eq | duckdb | 446.327 | 0.08× | validated |
| filter_eq | pandas | 2298.323 | 0.39× | validated |
| text_substr | lin | 5957.979 | 1.00× | validated |
| text_substr | sqlite | 3359.979 | 0.56× | validated |
| text_substr | duckdb | 500.704 | 0.08× | validated |
| text_substr | pandas | 4743.271 | 0.80× | validated |
| materialize | lin | 5685.395 | 1.00× | validated |
| materialize | sqlite | 20257.270 | 3.56× | validated |
| materialize | duckdb | 12939.291 | 2.28× | validated |
| materialize | pandas | 19014.084 | 3.34× | validated |
| join_inner | lin | 16672.855 | 1.00× | validated |
| join_inner | sqlite | 51496.667 | 3.09× | validated |
| join_inner | duckdb | 30121.979 | 1.81× | validated |
| join_inner | pandas | 38078.562 | 2.28× | validated |
| join_filter | lin | 7546.729 | 1.00× | validated |
| join_filter | sqlite | 26301.291 | 3.49× | validated |
| join_filter | duckdb | 14743.646 | 1.95× | validated |
| join_filter | pandas | 20026.042 | 2.65× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
