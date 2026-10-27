# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.914 | 8.04× | validated |
| point_get | duckdb | 40.636 | 357.50× | validated |
| point_get | postgres | 231.702 | 2038.43× | validated |
| point_get | mysql | 331.640 | 2917.65× | validated |
| point_get | mongo | 401.620 | 3533.32× | validated |
| point_get | pandas | 4.384 | 38.57× | validated |
| point_get | lin | 0.114 | 1.00× | validated |
| filter_eq | sqlite | 68.029 | 100.61× | validated |
| filter_eq | duckdb | 185.636 | 274.54× | validated |
| filter_eq | postgres | 443.284 | 655.57× | validated |
| filter_eq | mysql | 614.723 | 909.11× | validated |
| filter_eq | mongo | 885.300 | 1309.27× | validated |
| filter_eq | pandas | 275.727 | 407.77× | validated |
| filter_eq | lin | 0.676 | 1.00× | validated |
| text_substr | sqlite | 315.611 | 17.02× | validated |
| text_substr | duckdb | 99.181 | 5.35× | validated |
| text_substr | postgres | 733.007 | 39.53× | validated |
| text_substr | mysql | 1260.125 | 67.96× | validated |
| text_substr | mongo | 3102.459 | 167.33× | validated |
| text_substr | pandas | 482.873 | 26.04× | validated |
| text_substr | lin | 18.541 | 1.00× | validated |
| materialize | sqlite | 1828.229 | 4.84× | validated |
| materialize | duckdb | 1248.146 | 3.30× | validated |
| materialize | postgres | 1713.802 | 4.54× | validated |
| materialize | mysql | 10255.917 | 27.14× | validated |
| materialize | mongo | 5313.083 | 14.06× | validated |
| materialize | pandas | 1995.531 | 5.28× | validated |
| materialize | lin | 377.835 | 1.00× | validated |
| join_inner | sqlite | 4422.458 | 3.32× | validated |
| join_inner | duckdb | 2691.271 | 2.02× | validated |
| join_inner | postgres | 3925.521 | 2.95× | validated |
| join_inner | mysql | 20618.875 | 15.50× | validated |
| join_inner | mongo | 117708.896 | 88.49× | validated |
| join_inner | pandas | 3892.562 | 2.93× | validated |
| join_inner | lin | 1330.236 | 1.00× | validated |
| join_filter | sqlite | 2343.635 | 3.58× | validated |
| join_filter | duckdb | 1405.604 | 2.15× | validated |
| join_filter | postgres | 2251.406 | 3.44× | validated |
| join_filter | mysql | 11159.124 | 17.05× | validated |
| join_filter | mongo | 62265.021 | 95.16× | validated |
| join_filter | pandas | 2269.469 | 3.47× | validated |
| join_filter | lin | 654.351 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
