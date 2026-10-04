# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.654 | 1.00× | validated |
| point_get | sqlite | 2.753 | 4.21× | validated |
| point_get | duckdb | 50.562 | 77.29× | validated |
| point_get | postgres | 815.600 | 1246.79× | validated |
| point_get | mysql | 508.266 | 776.97× | validated |
| point_get | mongo | 559.562 | 855.39× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.308 | 9.64× | validated |
| filter_eq | lin | 1.729 | 1.00× | validated |
| filter_eq | sqlite | 1824.792 | 1055.25× | validated |
| filter_eq | duckdb | 604.941 | 349.83× | validated |
| filter_eq | postgres | 6108.604 | 3532.51× | validated |
| filter_eq | mysql | 5920.125 | 3423.52× | validated |
| filter_eq | mongo | 5855.542 | 3386.17× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 3249.542 | 1879.16× | validated |
| text_substr | lin | 594.308 | 1.00× | validated |
| text_substr | sqlite | 8011.771 | 13.48× | validated |
| text_substr | duckdb | 685.768 | 1.15× | validated |
| text_substr | postgres | 10056.187 | 16.92× | validated |
| text_substr | mysql | 15141.500 | 25.48× | validated |
| text_substr | mongo | 34080.626 | 57.35× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 6580.083 | 11.07× | validated |
| materialize | lin | 11101.500 | 1.00× | validated |
| materialize | sqlite | 49893.041 | 4.49× | validated |
| materialize | duckdb | 17471.875 | 1.57× | validated |
| materialize | postgres | 38834.938 | 3.50× | validated |
| materialize | mysql | 126736.270 | 11.42× | validated |
| materialize | mongo | 60492.562 | 5.45× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 24110.438 | 2.17× | validated |
| join_inner | lin | 32201.437 | 1.00× | validated |
| join_inner | sqlite | 112474.667 | 3.49× | validated |
| join_inner | duckdb | 40759.730 | 1.27× | validated |
| join_inner | postgres | 89927.417 | 2.79× | validated |
| join_inner | mysql | 281144.834 | 8.73× | validated |
| join_inner | mongo | 1604608.021 | 49.83× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 50187.376 | 1.56× | validated |
| join_filter | lin | 17884.292 | 1.00× | validated |
| join_filter | sqlite | 51069.104 | 2.86× | validated |
| join_filter | duckdb | 20684.438 | 1.16× | validated |
| join_filter | postgres | 46608.792 | 2.61× | validated |
| join_filter | mysql | 130725.958 | 7.31× | validated |
| join_filter | mongo | 800984.397 | 44.79× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 26087.541 | 1.46× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
