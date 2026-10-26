# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.113 | 1.00× | validated |
| point_get | sqlite | 0.992 | 8.77× | validated |
| point_get | duckdb | 41.705 | 368.65× | validated |
| point_get | postgres | 264.476 | 2337.86× | validated |
| point_get | mysql | 345.332 | 3052.60× | validated |
| point_get | mongo | 469.795 | 4152.81× | validated |
| point_get | pandas | 4.444 | 39.29× | validated |
| filter_eq | lin | 0.866 | 1.00× | validated |
| filter_eq | sqlite | 685.586 | 791.52× | validated |
| filter_eq | duckdb | 451.968 | 521.80× | validated |
| filter_eq | postgres | 2492.375 | 2877.47× | validated |
| filter_eq | mysql | 3564.854 | 4115.66× | validated |
| filter_eq | mongo | 5351.208 | 6178.03× | validated |
| filter_eq | pandas | 2344.271 | 2706.48× | validated |
| text_substr | lin | 206.425 | 1.00× | validated |
| text_substr | sqlite | 3505.104 | 16.98× | validated |
| text_substr | duckdb | 495.560 | 2.40× | validated |
| text_substr | postgres | 5667.437 | 27.46× | validated |
| text_substr | mysql | 9929.646 | 48.10× | validated |
| text_substr | mongo | 28304.687 | 137.12× | validated |
| text_substr | pandas | 5111.292 | 24.76× | validated |
| materialize | lin | 7008.083 | 1.00× | validated |
| materialize | sqlite | 21005.334 | 3.00× | validated |
| materialize | duckdb | 12852.917 | 1.83× | validated |
| materialize | postgres | 18374.854 | 2.62× | validated |
| materialize | mysql | 84388.105 | 12.04× | validated |
| materialize | mongo | 44275.459 | 6.32× | validated |
| materialize | pandas | 19079.313 | 2.72× | validated |
| join_inner | lin | 16661.188 | 1.00× | validated |
| join_inner | sqlite | 52486.917 | 3.15× | validated |
| join_inner | duckdb | 29611.355 | 1.78× | validated |
| join_inner | postgres | 48391.354 | 2.90× | validated |
| join_inner | mysql | 172402.729 | 10.35× | validated |
| join_inner | mongo | 1169392.188 | 70.19× | validated |
| join_inner | pandas | 38359.229 | 2.30× | validated |
| join_filter | lin | 8432.250 | 1.00× | validated |
| join_filter | sqlite | 26318.562 | 3.12× | validated |
| join_filter | duckdb | 14525.875 | 1.72× | validated |
| join_filter | postgres | 22672.417 | 2.69× | validated |
| join_filter | mysql | 91518.354 | 10.85× | validated |
| join_filter | mongo | 615432.229 | 72.99× | validated |
| join_filter | pandas | 19947.750 | 2.37× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
