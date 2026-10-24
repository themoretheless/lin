# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.953 | 8.41× | validated |
| point_get | duckdb | 40.605 | 358.19× | validated |
| point_get | postgres | 212.505 | 1874.57× | validated |
| point_get | mysql | 259.356 | 2287.84× | validated |
| point_get | mongo | 412.715 | 3640.66× | validated |
| point_get | pandas | 4.562 | 40.24× | validated |
| point_get | lin | 0.113 | 1.00× | validated |
| filter_eq | sqlite | 672.054 | 812.17× | validated |
| filter_eq | duckdb | 424.363 | 512.84× | validated |
| filter_eq | postgres | 2427.542 | 2933.65× | validated |
| filter_eq | mysql | 3461.542 | 4183.22× | validated |
| filter_eq | mongo | 5139.708 | 6211.26× | validated |
| filter_eq | pandas | 2304.573 | 2785.04× | validated |
| filter_eq | lin | 0.827 | 1.00× | validated |
| text_substr | sqlite | 3319.375 | 16.17× | validated |
| text_substr | duckdb | 467.169 | 2.28× | validated |
| text_substr | postgres | 5200.083 | 25.32× | validated |
| text_substr | mysql | 9587.250 | 46.69× | validated |
| text_substr | mongo | 27200.125 | 132.46× | validated |
| text_substr | pandas | 4745.458 | 23.11× | validated |
| text_substr | lin | 205.343 | 1.00× | validated |
| materialize | sqlite | 19585.313 | 3.42× | validated |
| materialize | duckdb | 12004.875 | 2.10× | validated |
| materialize | postgres | 14367.271 | 2.51× | validated |
| materialize | mysql | 83219.438 | 14.53× | validated |
| materialize | mongo | 43345.521 | 7.57× | validated |
| materialize | pandas | 18069.792 | 3.16× | validated |
| materialize | lin | 5725.979 | 1.00× | validated |
| join_inner | sqlite | 50015.666 | 3.15× | validated |
| join_inner | duckdb | 28475.770 | 1.80× | validated |
| join_inner | postgres | 40254.333 | 2.54× | validated |
| join_inner | mysql | 168510.750 | 10.63× | validated |
| join_inner | mongo | 1218022.771 | 76.81× | validated |
| join_inner | pandas | 36993.876 | 2.33× | validated |
| join_inner | lin | 15857.124 | 1.00× | validated |
| join_filter | sqlite | 24598.666 | 3.21× | validated |
| join_filter | duckdb | 13614.271 | 1.78× | validated |
| join_filter | postgres | 22369.541 | 2.92× | validated |
| join_filter | mysql | 91871.271 | 11.98× | validated |
| join_filter | mongo | 599241.146 | 78.16× | validated |
| join_filter | pandas | 19571.084 | 2.55× | validated |
| join_filter | lin | 7667.167 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
