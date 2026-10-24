# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.949 | 7.62× | validated |
| point_get | duckdb | 39.995 | 321.25× | validated |
| point_get | postgres | 322.779 | 2592.60× | validated |
| point_get | mysql | 376.378 | 3023.12× | validated |
| point_get | mongo | 475.708 | 3820.95× | validated |
| point_get | pandas | 4.695 | 37.71× | validated |
| point_get | lin | 0.124 | 1.00× | validated |
| filter_eq | sqlite | 7.450 | 11.06× | validated |
| filter_eq | duckdb | 136.205 | 202.30× | validated |
| filter_eq | postgres | 330.638 | 491.08× | validated |
| filter_eq | mysql | 415.735 | 617.46× | validated |
| filter_eq | mongo | 611.896 | 908.81× | validated |
| filter_eq | pandas | 58.072 | 86.25× | validated |
| filter_eq | lin | 0.673 | 1.00× | validated |
| text_substr | sqlite | 33.231 | 17.94× | validated |
| text_substr | duckdb | 58.568 | 31.61× | validated |
| text_substr | postgres | 383.783 | 207.15× | validated |
| text_substr | mysql | 432.122 | 233.24× | validated |
| text_substr | mongo | 838.049 | 452.35× | validated |
| text_substr | pandas | 82.280 | 44.41× | validated |
| text_substr | lin | 1.853 | 1.00× | validated |
| materialize | sqlite | 168.690 | 4.47× | validated |
| materialize | duckdb | 292.602 | 7.74× | validated |
| materialize | postgres | 484.747 | 12.83× | validated |
| materialize | mysql | 1247.444 | 33.02× | validated |
| materialize | mongo | 1383.875 | 36.63× | validated |
| materialize | pandas | 405.137 | 10.72× | validated |
| materialize | lin | 37.780 | 1.00× | validated |
| join_inner | sqlite | 387.484 | 3.03× | validated |
| join_inner | duckdb | 359.885 | 2.81× | validated |
| join_inner | postgres | 703.958 | 5.50× | validated |
| join_inner | mysql | 2425.407 | 18.94× | validated |
| join_inner | mongo | 13689.750 | 106.93× | validated |
| join_inner | pandas | 757.472 | 5.92× | validated |
| join_inner | lin | 128.029 | 1.00× | validated |
| join_filter | sqlite | 199.976 | 3.09× | validated |
| join_filter | duckdb | 258.100 | 3.99× | validated |
| join_filter | postgres | 507.782 | 7.86× | validated |
| join_filter | mysql | 1444.271 | 22.35× | validated |
| join_filter | mongo | 8453.812 | 130.80× | validated |
| join_filter | pandas | 717.768 | 11.11× | validated |
| join_filter | lin | 64.633 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
