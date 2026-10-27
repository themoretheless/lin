# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 40.326 | 342.40× | validated |
| point_get | postgres | 253.569 | 2152.99× | validated |
| point_get | mysql | 374.840 | 3182.68× | validated |
| point_get | mongo | 485.000 | 4118.02× | validated |
| point_get | pandas | 4.555 | 38.68× | validated |
| point_get | lin | 0.118 | 1.00× | validated |
| point_get | sqlite | 1.026 | 8.71× | validated |
| filter_eq | duckdb | 426.324 | 493.53× | validated |
| filter_eq | postgres | 2375.583 | 2750.09× | validated |
| filter_eq | mysql | 3468.604 | 4015.42× | validated |
| filter_eq | mongo | 5229.792 | 6054.25× | validated |
| filter_eq | pandas | 2358.312 | 2730.09× | validated |
| filter_eq | lin | 0.864 | 1.00× | validated |
| filter_eq | sqlite | 672.902 | 778.98× | validated |
| text_substr | duckdb | 471.721 | 2.30× | validated |
| text_substr | postgres | 5313.938 | 25.95× | validated |
| text_substr | mysql | 9856.813 | 48.14× | validated |
| text_substr | mongo | 27114.834 | 132.43× | validated |
| text_substr | pandas | 4863.917 | 23.76× | validated |
| text_substr | lin | 204.750 | 1.00× | validated |
| text_substr | sqlite | 3489.271 | 17.04× | validated |
| materialize | duckdb | 12589.209 | 1.91× | validated |
| materialize | postgres | 17802.250 | 2.71× | validated |
| materialize | mysql | 84424.292 | 12.83× | validated |
| materialize | mongo | 43135.000 | 6.56× | validated |
| materialize | pandas | 18237.854 | 2.77× | validated |
| materialize | lin | 6578.145 | 1.00× | validated |
| materialize | sqlite | 21142.166 | 3.21× | validated |
| join_inner | duckdb | 29263.812 | 1.76× | validated |
| join_inner | postgres | 47305.979 | 2.85× | validated |
| join_inner | mysql | 167935.188 | 10.11× | validated |
| join_inner | mongo | 1135872.188 | 68.36× | validated |
| join_inner | pandas | 37379.125 | 2.25× | validated |
| join_inner | lin | 16616.062 | 1.00× | validated |
| join_inner | sqlite | 52311.041 | 3.15× | validated |
| join_filter | duckdb | 14471.750 | 1.76× | validated |
| join_filter | postgres | 25254.416 | 3.07× | validated |
| join_filter | mysql | 90482.605 | 11.01× | validated |
| join_filter | mongo | 594300.333 | 72.31× | validated |
| join_filter | pandas | 19782.042 | 2.41× | validated |
| join_filter | lin | 8219.084 | 1.00× | validated |
| join_filter | sqlite | 26712.959 | 3.25× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
