# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 1.036 | 4.18× | validated |
| point_get | duckdb | 35.663 | 144.07× | validated |
| point_get | pandas | 4.497 | 18.17× | validated |
| point_get | lin | 0.248 | 1.00× | validated |
| filter_eq | sqlite | 715.271 | 923.78× | validated |
| filter_eq | duckdb | 451.436 | 583.04× | validated |
| filter_eq | pandas | 2300.104 | 2970.62× | validated |
| filter_eq | lin | 0.774 | 1.00× | validated |
| text_substr | sqlite | 3524.042 | 14.40× | validated |
| text_substr | duckdb | 505.653 | 2.07× | validated |
| text_substr | pandas | 4958.875 | 20.26× | validated |
| text_substr | lin | 244.710 | 1.00× | validated |
| materialize | sqlite | 21051.000 | 3.63× | validated |
| materialize | duckdb | 12496.792 | 2.15× | validated |
| materialize | pandas | 17623.459 | 3.04× | validated |
| materialize | lin | 5802.625 | 1.00× | validated |
| join_inner | sqlite | 52053.125 | 3.64× | validated |
| join_inner | duckdb | 30452.916 | 2.13× | validated |
| join_inner | pandas | 36421.541 | 2.55× | validated |
| join_inner | lin | 14291.750 | 1.00× | validated |
| join_filter | sqlite | 25969.875 | 3.65× | validated |
| join_filter | duckdb | 14497.625 | 2.04× | validated |
| join_filter | pandas | 19789.375 | 2.78× | validated |
| join_filter | lin | 7117.292 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
