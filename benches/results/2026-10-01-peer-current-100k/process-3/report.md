# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 40.713 | 168.15× | validated |
| point_get | postgres | 276.692 | 1142.76× | validated |
| point_get | mysql | 344.296 | 1421.98× | validated |
| point_get | mongo | 468.449 | 1934.74× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 4.785 | 19.76× | validated |
| point_get | lin | 0.242 | 1.00× | validated |
| point_get | sqlite | 1.055 | 4.36× | validated |
| filter_eq | duckdb | 479.454 | 604.32× | validated |
| filter_eq | postgres | 2859.861 | 3604.66× | validated |
| filter_eq | mysql | 3970.000 | 5003.91× | validated |
| filter_eq | mongo | 4232.625 | 5334.93× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 2306.010 | 2906.57× | validated |
| filter_eq | lin | 0.793 | 1.00× | validated |
| filter_eq | sqlite | 693.475 | 874.08× | validated |
| text_substr | duckdb | 516.357 | 2.49× | validated |
| text_substr | postgres | 6339.917 | 30.61× | validated |
| text_substr | mysql | 10641.041 | 51.37× | validated |
| text_substr | mongo | 24116.417 | 116.43× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 4946.625 | 23.88× | validated |
| text_substr | lin | 207.125 | 1.00× | validated |
| text_substr | sqlite | 3530.521 | 17.05× | validated |
| materialize | duckdb | 13258.291 | 2.20× | validated |
| materialize | postgres | 16232.959 | 2.69× | validated |
| materialize | mysql | 91822.292 | 15.23× | validated |
| materialize | mongo | 48193.125 | 7.99× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 18288.750 | 3.03× | validated |
| materialize | lin | 6028.625 | 1.00× | validated |
| materialize | sqlite | 22053.833 | 3.66× | validated |
| join_inner | duckdb | 29760.167 | 2.07× | validated |
| join_inner | postgres | 42348.583 | 2.94× | validated |
| join_inner | mysql | 178857.708 | 12.42× | validated |
| join_inner | mongo | 1163426.917 | 80.80× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 37004.625 | 2.57× | validated |
| join_inner | lin | 14398.667 | 1.00× | validated |
| join_inner | sqlite | 52585.208 | 3.65× | validated |
| join_filter | duckdb | 14657.042 | 2.03× | validated |
| join_filter | postgres | 22232.000 | 3.07× | validated |
| join_filter | mysql | 103737.958 | 14.34× | validated |
| join_filter | mongo | 615641.375 | 85.11× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 19264.625 | 2.66× | validated |
| join_filter | lin | 7233.333 | 1.00× | validated |
| join_filter | sqlite | 27394.541 | 3.79× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
