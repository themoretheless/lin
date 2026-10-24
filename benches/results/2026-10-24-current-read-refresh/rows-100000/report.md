# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.116 | 1.00× | validated |
| point_get | sqlite | 0.999 | 8.61× | validated |
| point_get | duckdb | 40.585 | 349.63× | validated |
| point_get | pandas | 4.484 | 38.63× | validated |
| filter_eq | lin | 5940.604 | 1.00× | validated |
| filter_eq | sqlite | 692.587 | 0.12× | validated |
| filter_eq | duckdb | 446.327 | 0.08× | validated |
| filter_eq | pandas | 2298.323 | 0.39× | validated |
| text_substr | lin | 6087.333 | 1.00× | validated |
| text_substr | sqlite | 3359.979 | 0.55× | validated |
| text_substr | duckdb | 487.109 | 0.08× | validated |
| text_substr | pandas | 4743.271 | 0.78× | validated |
| materialize | lin | 5685.395 | 1.00× | validated |
| materialize | sqlite | 20257.270 | 3.56× | validated |
| materialize | duckdb | 12552.916 | 2.21× | validated |
| materialize | pandas | 18344.688 | 3.23× | validated |
| join_inner | lin | 16672.855 | 1.00× | validated |
| join_inner | sqlite | 51515.145 | 3.09× | validated |
| join_inner | duckdb | 28719.583 | 1.72× | validated |
| join_inner | pandas | 39103.166 | 2.35× | validated |
| join_filter | lin | 7858.438 | 1.00× | validated |
| join_filter | sqlite | 26301.291 | 3.35× | validated |
| join_filter | duckdb | 14500.833 | 1.85× | validated |
| join_filter | pandas | 20026.042 | 2.55× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
