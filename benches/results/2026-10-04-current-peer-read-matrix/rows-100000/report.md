# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.654 | 1.00× | validated |
| point_get | sqlite | 1.952 | 2.98× | validated |
| point_get | duckdb | 50.562 | 77.29× | validated |
| point_get | postgres | 572.190 | 874.69× | validated |
| point_get | mysql | 508.266 | 776.97× | validated |
| point_get | mongo | 559.562 | 855.39× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.308 | 9.64× | validated |
| filter_eq | lin | 1.688 | 1.00× | validated |
| filter_eq | sqlite | 1297.646 | 768.83× | validated |
| filter_eq | duckdb | 604.941 | 358.42× | validated |
| filter_eq | postgres | 4098.896 | 2428.51× | validated |
| filter_eq | mysql | 5311.062 | 3146.70× | validated |
| filter_eq | mongo | 5855.542 | 3469.29× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 3249.542 | 1925.29× | validated |
| text_substr | lin | 502.368 | 1.00× | validated |
| text_substr | sqlite | 8011.771 | 15.95× | validated |
| text_substr | duckdb | 685.768 | 1.37× | validated |
| text_substr | postgres | 9152.938 | 18.22× | validated |
| text_substr | mysql | 15141.500 | 30.14× | validated |
| text_substr | mongo | 34080.626 | 67.84× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 6580.083 | 13.10× | validated |
| materialize | lin | 11101.500 | 1.00× | validated |
| materialize | sqlite | 47016.959 | 4.24× | validated |
| materialize | duckdb | 17471.875 | 1.57× | validated |
| materialize | postgres | 31799.542 | 2.86× | validated |
| materialize | mysql | 126736.270 | 11.42× | validated |
| materialize | mongo | 60492.562 | 5.45× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 24208.084 | 2.18× | validated |
| join_inner | lin | 26440.583 | 1.00× | validated |
| join_inner | sqlite | 112474.667 | 4.25× | validated |
| join_inner | duckdb | 40759.730 | 1.54× | validated |
| join_inner | postgres | 89927.417 | 3.40× | validated |
| join_inner | mysql | 281144.834 | 10.63× | validated |
| join_inner | mongo | 1604608.021 | 60.69× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 50187.376 | 1.90× | validated |
| join_filter | lin | 17884.292 | 1.00× | validated |
| join_filter | sqlite | 51069.104 | 2.86× | validated |
| join_filter | duckdb | 20684.438 | 1.16× | validated |
| join_filter | postgres | 46608.792 | 2.61× | validated |
| join_filter | mysql | 130725.958 | 7.31× | validated |
| join_filter | mongo | 826456.833 | 46.21× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 26087.541 | 1.46× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
