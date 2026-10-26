# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.122 | 1.00× | validated |
| point_get | sqlite | 0.971 | 7.98× | validated |
| point_get | duckdb | 41.705 | 342.98× | validated |
| point_get | postgres | 280.273 | 2304.96× | validated |
| point_get | mysql | 364.714 | 2999.39× | validated |
| point_get | mongo | 514.854 | 4234.14× | validated |
| point_get | pandas | 4.444 | 36.55× | validated |
| filter_eq | lin | 0.910 | 1.00× | validated |
| filter_eq | sqlite | 681.458 | 749.07× | validated |
| filter_eq | duckdb | 455.551 | 500.75× | validated |
| filter_eq | postgres | 3092.896 | 3399.74× | validated |
| filter_eq | mysql | 3758.166 | 4131.01× | validated |
| filter_eq | mongo | 5561.500 | 6113.25× | validated |
| filter_eq | pandas | 2352.062 | 2585.41× | validated |
| text_substr | lin | 204.213 | 1.00× | validated |
| text_substr | sqlite | 3452.813 | 16.91× | validated |
| text_substr | duckdb | 500.345 | 2.45× | validated |
| text_substr | postgres | 5699.292 | 27.91× | validated |
| text_substr | mysql | 10114.979 | 49.53× | validated |
| text_substr | mongo | 28304.687 | 138.60× | validated |
| text_substr | pandas | 5111.292 | 25.03× | validated |
| materialize | lin | 7008.083 | 1.00× | validated |
| materialize | sqlite | 20677.875 | 2.95× | validated |
| materialize | duckdb | 12794.876 | 1.83× | validated |
| materialize | postgres | 18374.854 | 2.62× | validated |
| materialize | mysql | 87177.604 | 12.44× | validated |
| materialize | mongo | 44242.916 | 6.31× | validated |
| materialize | pandas | 18465.812 | 2.63× | validated |
| join_inner | lin | 16825.562 | 1.00× | validated |
| join_inner | sqlite | 52196.645 | 3.10× | validated |
| join_inner | duckdb | 29611.355 | 1.76× | validated |
| join_inner | postgres | 42423.021 | 2.52× | validated |
| join_inner | mysql | 181017.959 | 10.76× | validated |
| join_inner | mongo | 1169840.125 | 69.53× | validated |
| join_inner | pandas | 37676.896 | 2.24× | validated |
| join_filter | lin | 8432.250 | 1.00× | validated |
| join_filter | sqlite | 26084.396 | 3.09× | validated |
| join_filter | duckdb | 15014.812 | 1.78× | validated |
| join_filter | postgres | 22300.938 | 2.64× | validated |
| join_filter | mysql | 97380.667 | 11.55× | validated |
| join_filter | mongo | 584975.083 | 69.37× | validated |
| join_filter | pandas | 19986.729 | 2.37× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
