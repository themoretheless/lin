# Validated read API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.122 | 1.00× | validated |
| point_get | sqlite | 0.952 | 7.80× | validated |
| point_get | duckdb | 39.640 | 324.75× | validated |
| point_get | postgres | 280.714 | 2299.72× | validated |
| point_get | mysql | 348.835 | 2857.79× | validated |
| point_get | mongo | 455.505 | 3731.67× | validated |
| point_get | pandas | 4.522 | 37.04× | validated |
| filter_eq | lin | 0.697 | 1.00× | validated |
| filter_eq | sqlite | 68.644 | 98.47× | validated |
| filter_eq | duckdb | 194.188 | 278.56× | validated |
| filter_eq | postgres | 488.081 | 700.14× | validated |
| filter_eq | mysql | 690.905 | 991.08× | validated |
| filter_eq | mongo | 982.203 | 1408.94× | validated |
| filter_eq | pandas | 265.651 | 381.07× | validated |
| text_substr | lin | 16.252 | 1.00× | validated |
| text_substr | sqlite | 328.971 | 20.24× | validated |
| text_substr | duckdb | 105.150 | 6.47× | validated |
| text_substr | postgres | 776.854 | 47.80× | validated |
| text_substr | mysql | 1306.820 | 80.41× | validated |
| text_substr | mongo | 3248.166 | 199.86× | validated |
| text_substr | pandas | 499.544 | 30.74× | validated |
| materialize | lin | 382.319 | 1.00× | validated |
| materialize | sqlite | 1858.802 | 4.86× | validated |
| materialize | duckdb | 1262.844 | 3.30× | validated |
| materialize | postgres | 1911.833 | 5.00× | validated |
| materialize | mysql | 9827.209 | 25.70× | validated |
| materialize | mongo | 5409.708 | 14.15× | validated |
| materialize | pandas | 1908.114 | 4.99× | validated |
| join_inner | lin | 1349.042 | 1.00× | validated |
| join_inner | sqlite | 4522.895 | 3.35× | validated |
| join_inner | duckdb | 2627.188 | 1.95× | validated |
| join_inner | postgres | 4628.833 | 3.43× | validated |
| join_inner | mysql | 20772.104 | 15.40× | validated |
| join_inner | mongo | 115275.583 | 85.45× | validated |
| join_inner | pandas | 4022.854 | 2.98× | validated |
| join_filter | lin | 678.141 | 1.00× | validated |
| join_filter | sqlite | 2327.250 | 3.43× | validated |
| join_filter | duckdb | 1395.229 | 2.06× | validated |
| join_filter | postgres | 2337.709 | 3.45× | validated |
| join_filter | mysql | 11034.354 | 16.27× | validated |
| join_filter | mongo | 59633.500 | 87.94× | validated |
| join_filter | pandas | 2397.792 | 3.54× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
