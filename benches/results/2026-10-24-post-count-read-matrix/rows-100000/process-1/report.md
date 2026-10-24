# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.123 | 1.00× | validated |
| point_get | sqlite | 0.982 | 7.97× | validated |
| point_get | duckdb | 41.284 | 335.20× | validated |
| point_get | pandas | 4.606 | 37.40× | validated |
| filter_eq | lin | 0.955 | 1.00× | validated |
| filter_eq | sqlite | 712.506 | 746.24× | validated |
| filter_eq | duckdb | 456.096 | 477.69× | validated |
| filter_eq | pandas | 2284.062 | 2392.21× | validated |
| text_substr | lin | 218.274 | 1.00× | validated |
| text_substr | sqlite | 3444.562 | 15.78× | validated |
| text_substr | duckdb | 502.910 | 2.30× | validated |
| text_substr | pandas | 4865.876 | 22.29× | validated |
| materialize | lin | 5732.145 | 1.00× | validated |
| materialize | sqlite | 21204.667 | 3.70× | validated |
| materialize | duckdb | 14043.771 | 2.45× | validated |
| materialize | pandas | 18516.000 | 3.23× | validated |
| join_inner | lin | 17188.520 | 1.00× | validated |
| join_inner | sqlite | 54282.375 | 3.16× | validated |
| join_inner | duckdb | 29638.896 | 1.72× | validated |
| join_inner | pandas | 38212.417 | 2.22× | validated |
| join_filter | lin | 8076.438 | 1.00× | validated |
| join_filter | sqlite | 27153.625 | 3.36× | validated |
| join_filter | duckdb | 15133.771 | 1.87× | validated |
| join_filter | pandas | 19767.562 | 2.45× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
