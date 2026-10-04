# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 46.135 | 117.35× | validated |
| point_get | postgres | 314.640 | 800.36× | validated |
| point_get | mysql | 360.161 | 916.15× | validated |
| point_get | mongo | 483.727 | 1230.47× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 6.174 | 15.70× | validated |
| point_get | lin | 0.393 | 1.00× | validated |
| point_get | sqlite | 1.308 | 3.33× | validated |
| filter_eq | duckdb | 173.151 | 181.08× | validated |
| filter_eq | postgres | 352.616 | 368.75× | validated |
| filter_eq | mysql | 403.693 | 422.17× | validated |
| filter_eq | mongo | 583.740 | 610.45× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 80.176 | 83.84× | validated |
| filter_eq | lin | 0.956 | 1.00× | validated |
| filter_eq | sqlite | 10.690 | 11.18× | validated |
| text_substr | duckdb | 76.722 | 26.66× | validated |
| text_substr | postgres | 376.133 | 130.68× | validated |
| text_substr | mysql | 489.260 | 169.99× | validated |
| text_substr | mongo | 967.621 | 336.19× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 106.803 | 37.11× | validated |
| text_substr | lin | 2.878 | 1.00× | validated |
| text_substr | sqlite | 47.301 | 16.43× | validated |
| materialize | duckdb | 398.965 | 6.86× | validated |
| materialize | postgres | 503.893 | 8.66× | validated |
| materialize | mysql | 1725.761 | 29.65× | validated |
| materialize | mongo | 1508.785 | 25.92× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 526.822 | 9.05× | validated |
| materialize | lin | 58.199 | 1.00× | validated |
| materialize | sqlite | 236.935 | 4.07× | validated |
| join_inner | duckdb | 502.391 | 2.83× | validated |
| join_inner | postgres | 793.167 | 4.47× | validated |
| join_inner | mysql | 3131.729 | 17.66× | validated |
| join_inner | mongo | 17973.334 | 101.37× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 1056.078 | 5.96× | validated |
| join_inner | lin | 177.297 | 1.00× | validated |
| join_inner | sqlite | 549.435 | 3.10× | validated |
| join_filter | duckdb | 334.313 | 3.72× | validated |
| join_filter | postgres | 585.112 | 6.52× | validated |
| join_filter | mysql | 1776.958 | 19.79× | validated |
| join_filter | mongo | 10445.624 | 116.35× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 871.729 | 9.71× | validated |
| join_filter | lin | 89.776 | 1.00× | validated |
| join_filter | sqlite | 287.581 | 3.20× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
