# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 42.616 | 173.57× | validated |
| point_get | postgres | 290.336 | 1182.49× | validated |
| point_get | mysql | 352.598 | 1436.07× | validated |
| point_get | mongo | 474.523 | 1932.66× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.669 | 19.02× | validated |
| point_get | lin | 0.246 | 1.00× | validated |
| point_get | sqlite | 1.036 | 4.22× | validated |
| filter_eq | duckdb | 442.046 | 649.97× | validated |
| filter_eq | postgres | 2466.083 | 3626.04× | validated |
| filter_eq | mysql | 3368.625 | 4953.10× | validated |
| filter_eq | mongo | 3901.834 | 5737.11× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 2285.208 | 3360.08× | validated |
| filter_eq | lin | 0.680 | 1.00× | validated |
| filter_eq | sqlite | 701.903 | 1032.05× | validated |
| text_substr | duckdb | 491.975 | 2.39× | validated |
| text_substr | postgres | 5511.667 | 26.82× | validated |
| text_substr | mysql | 9528.646 | 46.37× | validated |
| text_substr | mongo | 23740.541 | 115.53× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 4845.458 | 23.58× | validated |
| text_substr | lin | 205.496 | 1.00× | validated |
| text_substr | sqlite | 3477.812 | 16.92× | validated |
| materialize | duckdb | 13296.750 | 2.38× | validated |
| materialize | postgres | 18320.354 | 3.28× | validated |
| materialize | mysql | 93094.854 | 16.67× | validated |
| materialize | mongo | 44930.854 | 8.04× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 18209.896 | 3.26× | validated |
| materialize | lin | 5586.020 | 1.00× | validated |
| materialize | sqlite | 21216.666 | 3.80× | validated |
| join_inner | duckdb | 30561.500 | 2.16× | validated |
| join_inner | postgres | 39464.917 | 2.79× | validated |
| join_inner | mysql | 174468.229 | 12.32× | validated |
| join_inner | mongo | 1132299.479 | 79.96× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 38015.438 | 2.68× | validated |
| join_inner | lin | 14159.959 | 1.00× | validated |
| join_inner | sqlite | 51937.834 | 3.67× | validated |
| join_filter | duckdb | 14593.312 | 2.06× | validated |
| join_filter | postgres | 21661.791 | 3.06× | validated |
| join_filter | mysql | 93329.833 | 13.19× | validated |
| join_filter | mongo | 588559.688 | 83.15× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 19719.062 | 2.79× | validated |
| join_filter | lin | 7077.958 | 1.00× | validated |
| join_filter | sqlite | 26600.229 | 3.76× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
