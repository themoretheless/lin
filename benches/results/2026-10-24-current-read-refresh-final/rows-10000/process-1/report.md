# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.122 | 1.00× | validated |
| point_get | sqlite | 0.954 | 7.80× | validated |
| point_get | duckdb | 34.035 | 278.41× | validated |
| point_get | postgres | 184.302 | 1507.64× | validated |
| point_get | mysql | 198.245 | 1621.69× | validated |
| point_get | mongo | 330.578 | 2704.21× | validated |
| point_get | pandas | 4.438 | 36.31× | validated |
| filter_eq | lin | 0.690 | 1.00× | validated |
| filter_eq | sqlite | 66.886 | 96.94× | validated |
| filter_eq | duckdb | 176.984 | 256.50× | validated |
| filter_eq | postgres | 392.884 | 569.40× | validated |
| filter_eq | mysql | 510.588 | 739.98× | validated |
| filter_eq | mongo | 847.892 | 1228.83× | validated |
| filter_eq | pandas | 259.657 | 376.31× | validated |
| text_substr | lin | 16.104 | 1.00× | validated |
| text_substr | sqlite | 328.574 | 20.40× | validated |
| text_substr | duckdb | 88.859 | 5.52× | validated |
| text_substr | postgres | 663.658 | 41.21× | validated |
| text_substr | mysql | 1120.630 | 69.59× | validated |
| text_substr | mongo | 3068.812 | 190.56× | validated |
| text_substr | pandas | 475.817 | 29.55× | validated |
| materialize | lin | 373.546 | 1.00× | validated |
| materialize | sqlite | 1872.323 | 5.01× | validated |
| materialize | duckdb | 1275.639 | 3.41× | validated |
| materialize | postgres | 1736.010 | 4.65× | validated |
| materialize | mysql | 9468.334 | 25.35× | validated |
| materialize | mongo | 4925.229 | 13.19× | validated |
| materialize | pandas | 1865.979 | 5.00× | validated |
| join_inner | lin | 1290.549 | 1.00× | validated |
| join_inner | sqlite | 4511.667 | 3.50× | validated |
| join_inner | duckdb | 2598.438 | 2.01× | validated |
| join_inner | postgres | 4300.062 | 3.33× | validated |
| join_inner | mysql | 20359.688 | 15.78× | validated |
| join_inner | mongo | 118278.000 | 91.65× | validated |
| join_inner | pandas | 3934.979 | 3.05× | validated |
| join_filter | lin | 654.446 | 1.00× | validated |
| join_filter | sqlite | 2314.417 | 3.54× | validated |
| join_filter | duckdb | 1363.042 | 2.08× | validated |
| join_filter | postgres | 2609.479 | 3.99× | validated |
| join_filter | mysql | 10552.125 | 16.12× | validated |
| join_filter | mongo | 58770.792 | 89.80× | validated |
| join_filter | pandas | 2329.458 | 3.56× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
