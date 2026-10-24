# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 34.767 | 299.50× | validated |
| point_get | pandas | 4.409 | 37.98× | validated |
| point_get | lin | 0.116 | 1.00× | validated |
| point_get | sqlite | 0.997 | 8.59× | validated |
| filter_eq | duckdb | 437.379 | 0.07× | validated |
| filter_eq | pandas | 2211.948 | 0.37× | validated |
| filter_eq | lin | 5940.604 | 1.00× | validated |
| filter_eq | sqlite | 668.229 | 0.11× | validated |
| text_substr | duckdb | 467.685 | 0.08× | validated |
| text_substr | pandas | 4519.875 | 0.74× | validated |
| text_substr | lin | 6087.333 | 1.00× | validated |
| text_substr | sqlite | 3287.750 | 0.54× | validated |
| materialize | duckdb | 12552.916 | 2.51× | validated |
| materialize | pandas | 17153.500 | 3.43× | validated |
| materialize | lin | 5001.292 | 1.00× | validated |
| materialize | sqlite | 20089.480 | 4.02× | validated |
| join_inner | duckdb | 28084.541 | 1.70× | validated |
| join_inner | pandas | 39103.166 | 2.36× | validated |
| join_inner | lin | 16560.104 | 1.00× | validated |
| join_inner | sqlite | 51515.145 | 3.11× | validated |
| join_filter | duckdb | 13724.208 | 1.75× | validated |
| join_filter | pandas | 19373.291 | 2.47× | validated |
| join_filter | lin | 7858.438 | 1.00× | validated |
| join_filter | sqlite | 25737.146 | 3.28× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
