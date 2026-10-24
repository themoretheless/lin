# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 1.028 | 8.59× | validated |
| point_get | duckdb | 38.536 | 322.11× | validated |
| point_get | postgres | 253.163 | 2116.08× | validated |
| point_get | mysql | 330.667 | 2763.90× | validated |
| point_get | mongo | 415.076 | 3469.45× | validated |
| point_get | pandas | 4.431 | 37.04× | validated |
| point_get | lin | 0.120 | 1.00× | validated |
| filter_eq | sqlite | 73.769 | 112.37× | validated |
| filter_eq | duckdb | 187.298 | 285.31× | validated |
| filter_eq | postgres | 463.881 | 706.63× | validated |
| filter_eq | mysql | 644.578 | 981.89× | validated |
| filter_eq | mongo | 916.933 | 1396.77× | validated |
| filter_eq | pandas | 263.228 | 400.98× | validated |
| filter_eq | lin | 0.656 | 1.00× | validated |
| text_substr | sqlite | 335.562 | 20.01× | validated |
| text_substr | duckdb | 103.063 | 6.15× | validated |
| text_substr | postgres | 761.014 | 45.38× | validated |
| text_substr | mysql | 1297.222 | 77.35× | validated |
| text_substr | mongo | 3053.209 | 182.05× | validated |
| text_substr | pandas | 472.252 | 28.16× | validated |
| text_substr | lin | 16.771 | 1.00× | validated |
| materialize | sqlite | 1858.062 | 5.04× | validated |
| materialize | duckdb | 1243.875 | 3.37× | validated |
| materialize | postgres | 1590.875 | 4.31× | validated |
| materialize | mysql | 9598.458 | 26.02× | validated |
| materialize | mongo | 4969.000 | 13.47× | validated |
| materialize | pandas | 1787.896 | 4.85× | validated |
| materialize | lin | 368.939 | 1.00× | validated |
| join_inner | sqlite | 4435.750 | 3.45× | validated |
| join_inner | duckdb | 2689.729 | 2.09× | validated |
| join_inner | postgres | 3893.104 | 3.03× | validated |
| join_inner | mysql | 20484.438 | 15.93× | validated |
| join_inner | mongo | 112262.145 | 87.28× | validated |
| join_inner | pandas | 3847.771 | 2.99× | validated |
| join_inner | lin | 1286.201 | 1.00× | validated |
| join_filter | sqlite | 2336.010 | 3.49× | validated |
| join_filter | duckdb | 1447.139 | 2.16× | validated |
| join_filter | postgres | 2242.479 | 3.35× | validated |
| join_filter | mysql | 10667.312 | 15.93× | validated |
| join_filter | mongo | 57502.230 | 85.85× | validated |
| join_filter | pandas | 2206.781 | 3.29× | validated |
| join_filter | lin | 669.762 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
