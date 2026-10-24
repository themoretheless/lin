# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 308.722 | 1773.60× | validated |
| point_get | pandas | 40.747 | 234.09× | validated |
| point_get | lin | 0.174 | 1.00× | validated |
| point_get | sqlite | 0.934 | 5.37× | validated |
| filter_eq | duckdb | 753.796 | 12.94× | validated |
| filter_eq | pandas | 161.707 | 2.78× | validated |
| filter_eq | lin | 58.269 | 1.00× | validated |
| filter_eq | sqlite | 7.569 | 0.13× | validated |
| text_substr | duckdb | 676.302 | 15.45× | validated |
| text_substr | pandas | 468.706 | 10.71× | validated |
| text_substr | lin | 43.772 | 1.00× | validated |
| text_substr | sqlite | 32.073 | 0.73× | validated |
| materialize | duckdb | 2232.854 | 56.94× | validated |
| materialize | pandas | 1095.292 | 27.93× | validated |
| materialize | lin | 39.214 | 1.00× | validated |
| materialize | sqlite | 165.371 | 4.22× | validated |
| join_inner | duckdb | 2354.500 | 17.57× | validated |
| join_inner | pandas | 1504.042 | 11.23× | validated |
| join_inner | lin | 133.981 | 1.00× | validated |
| join_inner | sqlite | 385.627 | 2.88× | validated |
| join_filter | duckdb | 510.536 | 7.64× | validated |
| join_filter | pandas | 6962.230 | 104.19× | validated |
| join_filter | lin | 66.824 | 1.00× | validated |
| join_filter | sqlite | 205.251 | 3.07× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
