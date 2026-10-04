# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.1-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 100.221 | 153.11× | validated |
| point_get | postgres | 572.190 | 874.17× | validated |
| point_get | mysql | 585.573 | 894.62× | validated |
| point_get | mongo | 1065.437 | 1627.74× | validated |
| point_get | mssql | — | — | unavailable/error |
| point_get | kusto | — | — | unavailable/error |
| point_get | pandas | 9.965 | 15.22× | validated |
| point_get | lin | 0.655 | 1.00× | validated |
| point_get | sqlite | 1.952 | 2.98× | validated |
| filter_eq | duckdb | 948.758 | 562.12× | validated |
| filter_eq | postgres | 4098.896 | 2428.51× | validated |
| filter_eq | mysql | 5311.062 | 3146.70× | validated |
| filter_eq | mongo | 7122.687 | 4220.05× | validated |
| filter_eq | mssql | — | — | unavailable/error |
| filter_eq | kusto | — | — | unavailable/error |
| filter_eq | pandas | 4577.375 | 2712.00× | validated |
| filter_eq | lin | 1.688 | 1.00× | validated |
| filter_eq | sqlite | 1297.646 | 768.83× | validated |
| text_substr | duckdb | 906.179 | 1.80× | validated |
| text_substr | postgres | 9152.938 | 18.22× | validated |
| text_substr | mysql | 16170.271 | 32.19× | validated |
| text_substr | mongo | 39759.896 | 79.14× | validated |
| text_substr | mssql | — | — | unavailable/error |
| text_substr | kusto | — | — | unavailable/error |
| text_substr | pandas | 8068.105 | 16.06× | validated |
| text_substr | lin | 502.368 | 1.00× | validated |
| text_substr | sqlite | 8501.458 | 16.92× | validated |
| materialize | duckdb | 23660.188 | 1.81× | validated |
| materialize | postgres | 31799.542 | 2.44× | validated |
| materialize | mysql | 161253.250 | 12.36× | validated |
| materialize | mongo | 76231.667 | 5.84× | validated |
| materialize | mssql | — | — | unavailable/error |
| materialize | kusto | — | — | unavailable/error |
| materialize | pandas | 42878.605 | 3.29× | validated |
| materialize | lin | 13045.188 | 1.00× | validated |
| materialize | sqlite | 47016.959 | 3.60× | validated |
| join_inner | duckdb | 58360.417 | 2.21× | validated |
| join_inner | postgres | 96616.875 | 3.65× | validated |
| join_inner | mysql | 380077.605 | 14.37× | validated |
| join_inner | mongo | 1690539.604 | 63.94× | validated |
| join_inner | mssql | — | — | unavailable/error |
| join_inner | kusto | — | — | unavailable/error |
| join_inner | pandas | 65401.374 | 2.47× | validated |
| join_inner | lin | 26440.583 | 1.00× | validated |
| join_inner | sqlite | 154655.541 | 5.85× | validated |
| join_filter | duckdb | 21802.833 | 1.14× | validated |
| join_filter | postgres | 59599.813 | 3.12× | validated |
| join_filter | mysql | 169944.166 | 8.90× | validated |
| join_filter | mongo | 1089726.062 | 57.10× | validated |
| join_filter | mssql | — | — | unavailable/error |
| join_filter | kusto | — | — | unavailable/error |
| join_filter | pandas | 44331.854 | 2.32× | validated |
| join_filter | lin | 19085.105 | 1.00× | validated |
| join_filter | sqlite | 65138.145 | 3.41× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.28", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.53.4"}

Missing/failed peers are never counted as wins.
