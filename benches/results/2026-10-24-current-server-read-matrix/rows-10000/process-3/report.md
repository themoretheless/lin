# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 42.848 | 351.03× | validated |
| point_get | postgres | 1686.698 | 13818.08× | validated |
| point_get | mysql | 893.349 | 7318.66× | validated |
| point_get | mongo | 490.299 | 4016.72× | validated |
| point_get | pandas | 4.508 | 36.93× | validated |
| point_get | lin | 0.122 | 1.00× | validated |
| point_get | sqlite | 0.952 | 7.80× | validated |
| filter_eq | duckdb | 197.432 | 283.21× | validated |
| filter_eq | postgres | 3109.041 | 4459.83× | validated |
| filter_eq | mysql | 2003.365 | 2873.77× | validated |
| filter_eq | mongo | 982.203 | 1408.94× | validated |
| filter_eq | pandas | 265.651 | 381.07× | validated |
| filter_eq | lin | 0.697 | 1.00× | validated |
| filter_eq | sqlite | 68.644 | 98.47× | validated |
| text_substr | duckdb | 105.948 | 6.68× | validated |
| text_substr | postgres | 3566.959 | 224.95× | validated |
| text_substr | mysql | 4136.001 | 260.83× | validated |
| text_substr | mongo | 3268.250 | 206.11× | validated |
| text_substr | pandas | 496.882 | 31.34× | validated |
| text_substr | lin | 15.857 | 1.00× | validated |
| text_substr | sqlite | 328.971 | 20.75× | validated |
| materialize | duckdb | 1282.486 | 3.35× | validated |
| materialize | postgres | 7327.270 | 19.17× | validated |
| materialize | mysql | 10455.084 | 27.35× | validated |
| materialize | mongo | 5409.708 | 14.15× | validated |
| materialize | pandas | 1908.114 | 4.99× | validated |
| materialize | lin | 382.319 | 1.00× | validated |
| materialize | sqlite | 1936.073 | 5.06× | validated |
| join_inner | duckdb | 2627.188 | 1.95× | validated |
| join_inner | postgres | 8392.438 | 6.22× | validated |
| join_inner | mysql | 21158.125 | 15.68× | validated |
| join_inner | mongo | 116418.062 | 86.30× | validated |
| join_inner | pandas | 4022.854 | 2.98× | validated |
| join_inner | lin | 1349.042 | 1.00× | validated |
| join_inner | sqlite | 4544.708 | 3.37× | validated |
| join_filter | duckdb | 3706.333 | 5.39× | validated |
| join_filter | postgres | 6512.500 | 9.48× | validated |
| join_filter | mysql | 10877.854 | 15.83× | validated |
| join_filter | mongo | 59633.500 | 86.78× | validated |
| join_filter | pandas | 2405.469 | 3.50× | validated |
| join_filter | lin | 687.167 | 1.00× | validated |
| join_filter | sqlite | 2364.750 | 3.44× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
