# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.117 | 1.00× | validated |
| point_get | sqlite | 0.932 | 7.95× | validated |
| point_get | duckdb | 37.147 | 316.94× | validated |
| point_get | postgres | 232.035 | 1979.75× | validated |
| point_get | mysql | 327.425 | 2793.63× | validated |
| point_get | mongo | 410.155 | 3499.49× | validated |
| point_get | pandas | 4.593 | 39.19× | validated |
| filter_eq | lin | 0.664 | 1.00× | validated |
| filter_eq | sqlite | 68.285 | 102.80× | validated |
| filter_eq | duckdb | 185.851 | 279.80× | validated |
| filter_eq | postgres | 449.061 | 676.07× | validated |
| filter_eq | mysql | 631.077 | 950.10× | validated |
| filter_eq | mongo | 898.600 | 1352.86× | validated |
| filter_eq | pandas | 265.821 | 400.20× | validated |
| text_substr | lin | 15.612 | 1.00× | validated |
| text_substr | sqlite | 332.390 | 21.29× | validated |
| text_substr | duckdb | 102.618 | 6.57× | validated |
| text_substr | postgres | 737.365 | 47.23× | validated |
| text_substr | mysql | 1259.014 | 80.64× | validated |
| text_substr | mongo | 3064.021 | 196.26× | validated |
| text_substr | pandas | 482.361 | 30.90× | validated |
| materialize | lin | 382.737 | 1.00× | validated |
| materialize | sqlite | 1861.552 | 4.86× | validated |
| materialize | duckdb | 1219.188 | 3.19× | validated |
| materialize | postgres | 1863.261 | 4.87× | validated |
| materialize | mysql | 9619.208 | 25.13× | validated |
| materialize | mongo | 4981.750 | 13.02× | validated |
| materialize | pandas | 1909.719 | 4.99× | validated |
| join_inner | lin | 1262.646 | 1.00× | validated |
| join_inner | sqlite | 4391.771 | 3.48× | validated |
| join_inner | duckdb | 2568.791 | 2.03× | validated |
| join_inner | postgres | 4684.708 | 3.71× | validated |
| join_inner | mysql | 20258.916 | 16.04× | validated |
| join_inner | mongo | 357016.688 | 282.75× | validated |
| join_inner | pandas | 4033.084 | 3.19× | validated |
| join_filter | lin | 665.223 | 1.00× | validated |
| join_filter | sqlite | 2294.198 | 3.45× | validated |
| join_filter | duckdb | 1356.042 | 2.04× | validated |
| join_filter | postgres | 2308.125 | 3.47× | validated |
| join_filter | mysql | 10610.062 | 15.95× | validated |
| join_filter | mongo | 60267.979 | 90.60× | validated |
| join_filter | pandas | 2271.459 | 3.41× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
