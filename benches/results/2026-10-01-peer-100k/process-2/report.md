# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 1.513 | 6.41× | validated |
| point_get | duckdb | 44.235 | 187.47× | validated |
| point_get | postgres | 323.528 | 1371.15× | validated |
| point_get | mysql | 363.771 | 1541.70× | validated |
| point_get | mongo | 363.400 | 1540.13× | validated |
| point_get | pandas | 4.731 | 20.05× | validated |
| point_get | lin | 0.236 | 1.00× | validated |
| filter_eq | sqlite | 1273.635 | 1646.62× | validated |
| filter_eq | duckdb | 450.562 | 582.51× | validated |
| filter_eq | postgres | 2940.312 | 3801.38× | validated |
| filter_eq | mysql | 3459.229 | 4472.26× | validated |
| filter_eq | mongo | 4290.708 | 5547.24× | validated |
| filter_eq | pandas | 2338.187 | 3022.93× | validated |
| filter_eq | lin | 0.773 | 1.00× | validated |
| text_substr | sqlite | 4461.417 | 5.22× | validated |
| text_substr | duckdb | 488.704 | 0.57× | validated |
| text_substr | postgres | 5813.646 | 6.80× | validated |
| text_substr | mysql | 9781.646 | 11.43× | validated |
| text_substr | mongo | 23872.500 | 27.91× | validated |
| text_substr | pandas | 5227.645 | 6.11× | validated |
| text_substr | lin | 855.437 | 1.00× | validated |
| materialize | sqlite | 21355.959 | 3.36× | validated |
| materialize | duckdb | 12575.354 | 1.98× | validated |
| materialize | postgres | 15231.854 | 2.39× | validated |
| materialize | mysql | 89876.417 | 14.13× | validated |
| materialize | mongo | 45948.479 | 7.22× | validated |
| materialize | pandas | 21098.000 | 3.32× | validated |
| materialize | lin | 6361.042 | 1.00× | validated |
| join_inner | sqlite | 52727.166 | 3.57× | validated |
| join_inner | duckdb | 29379.479 | 1.99× | validated |
| join_inner | postgres | 41336.626 | 2.80× | validated |
| join_inner | mysql | 177279.771 | 12.02× | validated |
| join_inner | mongo | 1212882.229 | 82.23× | validated |
| join_inner | pandas | 37337.396 | 2.53× | validated |
| join_inner | lin | 14750.667 | 1.00× | validated |
| join_filter | sqlite | 26004.625 | 3.58× | validated |
| join_filter | duckdb | 14352.792 | 1.97× | validated |
| join_filter | postgres | 22274.188 | 3.06× | validated |
| join_filter | mysql | 94298.625 | 12.97× | validated |
| join_filter | mongo | 597402.688 | 82.17× | validated |
| join_filter | pandas | 19291.834 | 2.65× | validated |
| join_filter | lin | 7270.562 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
