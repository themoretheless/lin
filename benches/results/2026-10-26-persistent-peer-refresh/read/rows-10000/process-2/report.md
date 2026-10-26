# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.952 | 8.52× | validated |
| point_get | duckdb | 41.178 | 368.52× | validated |
| point_get | postgres | 263.750 | 2360.44× | validated |
| point_get | mysql | 337.304 | 3018.72× | validated |
| point_get | mongo | 446.279 | 3994.00× | validated |
| point_get | pandas | 4.556 | 40.78× | validated |
| point_get | lin | 0.112 | 1.00× | validated |
| filter_eq | sqlite | 67.759 | 97.48× | validated |
| filter_eq | duckdb | 194.461 | 279.75× | validated |
| filter_eq | postgres | 489.144 | 703.69× | validated |
| filter_eq | mysql | 673.027 | 968.23× | validated |
| filter_eq | mongo | 1053.026 | 1514.90× | validated |
| filter_eq | pandas | 261.403 | 376.06× | validated |
| filter_eq | lin | 0.695 | 1.00× | validated |
| text_substr | sqlite | 332.875 | 20.33× | validated |
| text_substr | duckdb | 106.680 | 6.51× | validated |
| text_substr | postgres | 811.316 | 49.54× | validated |
| text_substr | mysql | 1379.618 | 84.24× | validated |
| text_substr | mongo | 3459.604 | 211.25× | validated |
| text_substr | pandas | 493.273 | 30.12× | validated |
| text_substr | lin | 16.377 | 1.00× | validated |
| materialize | sqlite | 1856.750 | 4.84× | validated |
| materialize | duckdb | 1296.528 | 3.38× | validated |
| materialize | postgres | 1665.625 | 4.34× | validated |
| materialize | mysql | 10156.583 | 26.46× | validated |
| materialize | mongo | 5597.542 | 14.58× | validated |
| materialize | pandas | 1989.625 | 5.18× | validated |
| materialize | lin | 383.901 | 1.00× | validated |
| join_inner | sqlite | 4487.749 | 3.27× | validated |
| join_inner | duckdb | 2817.479 | 2.05× | validated |
| join_inner | postgres | 3991.416 | 2.91× | validated |
| join_inner | mysql | 20937.416 | 15.26× | validated |
| join_inner | mongo | 115171.562 | 83.95× | validated |
| join_inner | pandas | 4182.021 | 3.05× | validated |
| join_inner | lin | 1371.854 | 1.00× | validated |
| join_filter | sqlite | 2370.062 | 3.45× | validated |
| join_filter | duckdb | 1527.736 | 2.23× | validated |
| join_filter | postgres | 2325.260 | 3.39× | validated |
| join_filter | mysql | 11178.229 | 16.29× | validated |
| join_filter | mongo | 59942.875 | 87.37× | validated |
| join_filter | pandas | 2466.552 | 3.60× | validated |
| join_filter | lin | 686.054 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
