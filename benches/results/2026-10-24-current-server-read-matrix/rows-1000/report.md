# Validated read API benchmark

Rows: 1000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.120 | 1.00× | validated |
| point_get | sqlite | 0.941 | 7.82× | validated |
| point_get | duckdb | 39.972 | 332.05× | validated |
| point_get | postgres | 322.779 | 2681.35× | validated |
| point_get | mysql | 347.277 | 2884.86× | validated |
| point_get | mongo | 470.891 | 3911.73× | validated |
| point_get | pandas | 4.695 | 39.00× | validated |
| filter_eq | lin | 0.682 | 1.00× | validated |
| filter_eq | sqlite | 7.525 | 11.03× | validated |
| filter_eq | duckdb | 136.216 | 199.61× | validated |
| filter_eq | postgres | 340.510 | 498.97× | validated |
| filter_eq | mysql | 364.205 | 533.69× | validated |
| filter_eq | mongo | 542.185 | 794.50× | validated |
| filter_eq | pandas | 56.342 | 82.56× | validated |
| text_substr | lin | 1.927 | 1.00× | validated |
| text_substr | sqlite | 32.283 | 16.76× | validated |
| text_substr | duckdb | 59.179 | 30.72× | validated |
| text_substr | postgres | 363.364 | 188.60× | validated |
| text_substr | mysql | 416.361 | 216.11× | validated |
| text_substr | mongo | 822.094 | 426.71× | validated |
| text_substr | pandas | 76.567 | 39.74× | validated |
| materialize | lin | 37.780 | 1.00× | validated |
| materialize | sqlite | 168.690 | 4.47× | validated |
| materialize | duckdb | 292.602 | 7.74× | validated |
| materialize | postgres | 455.664 | 12.06× | validated |
| materialize | mysql | 1218.844 | 32.26× | validated |
| materialize | mongo | 1383.875 | 36.63× | validated |
| materialize | pandas | 379.943 | 10.06× | validated |
| join_inner | lin | 130.820 | 1.00× | validated |
| join_inner | sqlite | 387.484 | 2.96× | validated |
| join_inner | duckdb | 362.021 | 2.77× | validated |
| join_inner | postgres | 685.191 | 5.24× | validated |
| join_inner | mysql | 2360.052 | 18.04× | validated |
| join_inner | mongo | 12630.250 | 96.55× | validated |
| join_inner | pandas | 720.281 | 5.51× | validated |
| join_filter | lin | 64.633 | 1.00× | validated |
| join_filter | sqlite | 201.593 | 3.12× | validated |
| join_filter | duckdb | 245.580 | 3.80× | validated |
| join_filter | postgres | 507.782 | 7.86× | validated |
| join_filter | mysql | 1444.271 | 22.35× | validated |
| join_filter | mongo | 6663.896 | 103.10× | validated |
| join_filter | pandas | 627.080 | 9.70× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
