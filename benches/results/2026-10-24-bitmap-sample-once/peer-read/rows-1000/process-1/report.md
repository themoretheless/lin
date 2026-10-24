# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.161 | 1.00× | validated |
| point_get | sqlite | 0.921 | 5.71× | validated |
| point_get | duckdb | 39.532 | 245.19× | validated |
| point_get | postgres | 326.990 | 2028.13× | validated |
| point_get | mysql | 320.299 | 1986.63× | validated |
| point_get | mongo | 479.682 | 2975.19× | validated |
| point_get | pandas | 5.831 | 36.16× | validated |
| filter_eq | lin | 0.683 | 1.00× | validated |
| filter_eq | sqlite | 7.489 | 10.96× | validated |
| filter_eq | duckdb | 137.609 | 201.34× | validated |
| filter_eq | postgres | 338.763 | 495.66× | validated |
| filter_eq | mysql | 355.662 | 520.38× | validated |
| filter_eq | mongo | 492.823 | 721.07× | validated |
| filter_eq | pandas | 57.381 | 83.96× | validated |
| text_substr | lin | 1.607 | 1.00× | validated |
| text_substr | sqlite | 32.151 | 20.01× | validated |
| text_substr | duckdb | 57.141 | 35.56× | validated |
| text_substr | postgres | 351.310 | 218.61× | validated |
| text_substr | mysql | 413.684 | 257.42× | validated |
| text_substr | mongo | 721.402 | 448.90× | validated |
| text_substr | pandas | 72.441 | 45.08× | validated |
| materialize | lin | 43.840 | 1.00× | validated |
| materialize | sqlite | 163.335 | 3.73× | validated |
| materialize | duckdb | 292.384 | 6.67× | validated |
| materialize | postgres | 416.090 | 9.49× | validated |
| materialize | mysql | 1211.031 | 27.62× | validated |
| materialize | mongo | 1207.016 | 27.53× | validated |
| materialize | pandas | 362.524 | 8.27× | validated |
| join_inner | lin | 127.402 | 1.00× | validated |
| join_inner | sqlite | 385.965 | 3.03× | validated |
| join_inner | duckdb | 357.799 | 2.81× | validated |
| join_inner | postgres | 682.792 | 5.36× | validated |
| join_inner | mysql | 2797.896 | 21.96× | validated |
| join_inner | mongo | 13745.584 | 107.89× | validated |
| join_inner | pandas | 691.528 | 5.43× | validated |
| join_filter | lin | 68.442 | 1.00× | validated |
| join_filter | sqlite | 198.024 | 2.89× | validated |
| join_filter | duckdb | 251.831 | 3.68× | validated |
| join_filter | postgres | 499.956 | 7.30× | validated |
| join_filter | mysql | 1602.188 | 23.41× | validated |
| join_filter | mongo | 8529.354 | 124.62× | validated |
| join_filter | pandas | 617.076 | 9.02× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
