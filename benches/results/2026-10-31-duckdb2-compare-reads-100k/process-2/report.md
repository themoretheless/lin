# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.997 | 9.59× | validated |
| point_get | duckdb | 124.103 | 1194.16× | validated |
| point_get | lin | 0.104 | 1.00× | validated |
| filter_eq | sqlite | 694.220 | 896.99× | validated |
| filter_eq | duckdb | 349.340 | 451.38× | validated |
| filter_eq | lin | 0.774 | 1.00× | validated |
| text_substr | sqlite | 3569.166 | 17.27× | validated |
| text_substr | duckdb | 1496.931 | 7.24× | validated |
| text_substr | lin | 206.653 | 1.00× | validated |
| materialize | sqlite | 21353.708 | 3.16× | validated |
| materialize | duckdb | 14312.041 | 2.12× | validated |
| materialize | lin | 6751.625 | 1.00× | validated |
| join_inner | sqlite | 55416.083 | 3.49× | validated |
| join_inner | duckdb | 29146.417 | 1.84× | validated |
| join_inner | lin | 15881.791 | 1.00× | validated |
| join_filter | sqlite | 28357.625 | 3.39× | validated |
| join_filter | duckdb | 14900.833 | 1.78× | validated |
| join_filter | lin | 8353.458 | 1.00× | validated |

Versions: {"duckdb": "2.0.0", "lin": "0.4.0", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
