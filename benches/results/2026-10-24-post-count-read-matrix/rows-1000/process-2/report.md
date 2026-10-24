# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.909 | 7.80× | validated |
| point_get | duckdb | 40.042 | 343.52× | validated |
| point_get | pandas | 4.351 | 37.33× | validated |
| point_get | lin | 0.117 | 1.00× | validated |
| filter_eq | sqlite | 7.390 | 11.78× | validated |
| filter_eq | duckdb | 131.070 | 209.02× | validated |
| filter_eq | pandas | 55.775 | 88.94× | validated |
| filter_eq | lin | 0.627 | 1.00× | validated |
| text_substr | sqlite | 32.069 | 16.21× | validated |
| text_substr | duckdb | 58.683 | 29.67× | validated |
| text_substr | pandas | 73.344 | 37.08× | validated |
| text_substr | lin | 1.978 | 1.00× | validated |
| materialize | sqlite | 161.879 | 4.51× | validated |
| materialize | duckdb | 293.664 | 8.17× | validated |
| materialize | pandas | 359.292 | 10.00× | validated |
| materialize | lin | 35.929 | 1.00× | validated |
| join_inner | sqlite | 381.353 | 3.03× | validated |
| join_inner | duckdb | 345.870 | 2.75× | validated |
| join_inner | pandas | 702.681 | 5.59× | validated |
| join_inner | lin | 125.763 | 1.00× | validated |
| join_filter | sqlite | 202.139 | 3.20× | validated |
| join_filter | duckdb | 241.596 | 3.82× | validated |
| join_filter | pandas | 615.888 | 9.75× | validated |
| join_filter | lin | 63.163 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
