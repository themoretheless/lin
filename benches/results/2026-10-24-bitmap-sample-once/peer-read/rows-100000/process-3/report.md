# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 39.286 | 345.92× | validated |
| point_get | postgres | 245.926 | 2165.40× | validated |
| point_get | mysql | 325.790 | 2868.60× | validated |
| point_get | mongo | 414.110 | 3646.27× | validated |
| point_get | pandas | 4.389 | 38.64× | validated |
| point_get | lin | 0.114 | 1.00× | validated |
| point_get | sqlite | 0.987 | 8.69× | validated |
| filter_eq | duckdb | 441.748 | 502.50× | validated |
| filter_eq | postgres | 2322.427 | 2641.81× | validated |
| filter_eq | mysql | 3362.501 | 3824.92× | validated |
| filter_eq | mongo | 4820.896 | 5483.87× | validated |
| filter_eq | pandas | 2308.125 | 2625.54× | validated |
| filter_eq | lin | 0.879 | 1.00× | validated |
| filter_eq | sqlite | 675.979 | 768.94× | validated |
| text_substr | duckdb | 474.475 | 2.30× | validated |
| text_substr | postgres | 5344.833 | 25.88× | validated |
| text_substr | mysql | 9482.875 | 45.91× | validated |
| text_substr | mongo | 26633.437 | 128.95× | validated |
| text_substr | pandas | 4531.917 | 21.94× | validated |
| text_substr | lin | 206.543 | 1.00× | validated |
| text_substr | sqlite | 3356.416 | 16.25× | validated |
| materialize | duckdb | 12508.125 | 2.22× | validated |
| materialize | postgres | 14094.146 | 2.50× | validated |
| materialize | mysql | 82965.688 | 14.72× | validated |
| materialize | mongo | 42810.522 | 7.60× | validated |
| materialize | pandas | 16891.250 | 3.00× | validated |
| materialize | lin | 5635.896 | 1.00× | validated |
| materialize | sqlite | 20235.666 | 3.59× | validated |
| join_inner | duckdb | 29298.917 | 1.82× | validated |
| join_inner | postgres | 46008.020 | 2.85× | validated |
| join_inner | mysql | 166638.396 | 10.33× | validated |
| join_inner | mongo | 1123385.062 | 69.65× | validated |
| join_inner | pandas | 36161.978 | 2.24× | validated |
| join_inner | lin | 16129.583 | 1.00× | validated |
| join_inner | sqlite | 50465.750 | 3.13× | validated |
| join_filter | duckdb | 13936.167 | 1.92× | validated |
| join_filter | postgres | 24981.646 | 3.45× | validated |
| join_filter | mysql | 90642.000 | 12.50× | validated |
| join_filter | mongo | 565563.083 | 78.00× | validated |
| join_filter | pandas | 18590.750 | 2.56× | validated |
| join_filter | lin | 7250.438 | 1.00× | validated |
| join_filter | sqlite | 25725.187 | 3.55× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
