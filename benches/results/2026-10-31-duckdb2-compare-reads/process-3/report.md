# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 120.311 | 1061.24× | validated |
| point_get | lin | 0.113 | 1.00× | validated |
| point_get | sqlite | 0.964 | 8.50× | validated |
| filter_eq | duckdb | 258.368 | 459.87× | validated |
| filter_eq | lin | 0.562 | 1.00× | validated |
| filter_eq | sqlite | 68.529 | 121.98× | validated |
| text_substr | duckdb | 229.675 | 13.19× | validated |
| text_substr | lin | 17.410 | 1.00× | validated |
| text_substr | sqlite | 325.867 | 18.72× | validated |
| materialize | duckdb | 1508.153 | 4.08× | validated |
| materialize | lin | 369.442 | 1.00× | validated |
| materialize | sqlite | 1895.698 | 5.13× | validated |
| join_inner | duckdb | 3212.521 | 2.30× | validated |
| join_inner | lin | 1397.229 | 1.00× | validated |
| join_inner | sqlite | 4563.105 | 3.27× | validated |
| join_filter | duckdb | 1885.052 | 2.94× | validated |
| join_filter | lin | 640.719 | 1.00× | validated |
| join_filter | sqlite | 2320.500 | 3.62× | validated |

Versions: {"duckdb": "2.0.0", "lin": "0.4.0", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
