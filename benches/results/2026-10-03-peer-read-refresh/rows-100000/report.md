# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.246 | 1.00× | validated |
| point_get | sqlite | 1.013 | 4.12× | validated |
| point_get | duckdb | 40.013 | 162.97× | validated |
| point_get | postgres | 280.072 | 1140.69× | validated |
| point_get | mysql | 352.598 | 1436.07× | validated |
| point_get | mongo | 444.757 | 1811.42× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.669 | 19.02× | validated |
| filter_eq | lin | 0.738 | 1.00× | validated |
| filter_eq | sqlite | 701.903 | 951.18× | validated |
| filter_eq | duckdb | 438.163 | 593.77× | validated |
| filter_eq | postgres | 2466.083 | 3341.89× | validated |
| filter_eq | mysql | 3368.625 | 4564.96× | validated |
| filter_eq | mongo | 3901.834 | 5287.54× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 2318.792 | 3142.29× | validated |
| text_substr | lin | 208.178 | 1.00× | validated |
| text_substr | sqlite | 3364.834 | 16.16× | validated |
| text_substr | duckdb | 470.685 | 2.26× | validated |
| text_substr | postgres | 5268.812 | 25.31× | validated |
| text_substr | mysql | 9586.312 | 46.05× | validated |
| text_substr | mongo | 23740.541 | 114.04× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 4845.458 | 23.28× | validated |
| materialize | lin | 5556.021 | 1.00× | validated |
| materialize | sqlite | 20899.688 | 3.76× | validated |
| materialize | duckdb | 12406.896 | 2.23× | validated |
| materialize | postgres | 15143.000 | 2.73× | validated |
| materialize | mysql | 87109.583 | 15.68× | validated |
| materialize | mongo | 44930.854 | 8.09× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 17799.480 | 3.20× | validated |
| join_inner | lin | 14159.959 | 1.00× | validated |
| join_inner | sqlite | 51937.834 | 3.67× | validated |
| join_inner | duckdb | 29679.395 | 2.10× | validated |
| join_inner | postgres | 39920.084 | 2.82× | validated |
| join_inner | mysql | 174297.729 | 12.31× | validated |
| join_inner | mongo | 1124911.250 | 79.44× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 38015.438 | 2.68× | validated |
| join_filter | lin | 7077.958 | 1.00× | validated |
| join_filter | sqlite | 26031.896 | 3.68× | validated |
| join_filter | duckdb | 14270.688 | 2.02× | validated |
| join_filter | postgres | 21661.791 | 3.06× | validated |
| join_filter | mysql | 93329.833 | 13.19× | validated |
| join_filter | mongo | 571808.812 | 80.79× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 19478.375 | 2.75× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
