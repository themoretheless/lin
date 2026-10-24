# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.940 | 8.31× | validated |
| point_get | duckdb | 40.741 | 360.22× | validated |
| point_get | postgres | 1769.438 | 15644.60× | validated |
| point_get | mysql | 270.261 | 2389.53× | validated |
| point_get | mongo | 2631.708 | 23268.43× | validated |
| point_get | pandas | 4.307 | 38.08× | validated |
| point_get | lin | 0.113 | 1.00× | validated |
| filter_eq | sqlite | 672.961 | 809.24× | validated |
| filter_eq | duckdb | 426.953 | 513.42× | validated |
| filter_eq | postgres | 8989.021 | 10809.40× | validated |
| filter_eq | mysql | 3447.291 | 4145.41× | validated |
| filter_eq | mongo | 9864.417 | 11862.08× | validated |
| filter_eq | pandas | 2317.542 | 2786.87× | validated |
| filter_eq | lin | 0.832 | 1.00× | validated |
| text_substr | sqlite | 3263.229 | 16.07× | validated |
| text_substr | duckdb | 471.027 | 2.32× | validated |
| text_substr | postgres | 14407.146 | 70.95× | validated |
| text_substr | mysql | 9708.479 | 47.81× | validated |
| text_substr | mongo | 70761.042 | 348.48× | validated |
| text_substr | pandas | 4658.938 | 22.94× | validated |
| text_substr | lin | 203.057 | 1.00× | validated |
| materialize | sqlite | 20033.062 | 4.17× | validated |
| materialize | duckdb | 11869.771 | 2.47× | validated |
| materialize | postgres | 30632.000 | 6.38× | validated |
| materialize | mysql | 140223.021 | 29.20× | validated |
| materialize | mongo | 43776.459 | 9.12× | validated |
| materialize | pandas | 16795.145 | 3.50× | validated |
| materialize | lin | 4802.376 | 1.00× | validated |
| join_inner | sqlite | 50268.416 | 3.23× | validated |
| join_inner | duckdb | 27325.333 | 1.76× | validated |
| join_inner | postgres | 125900.312 | 8.10× | validated |
| join_inner | mysql | 431088.604 | 27.72× | validated |
| join_inner | mongo | 1263641.667 | 81.27× | validated |
| join_inner | pandas | 35957.895 | 2.31× | validated |
| join_inner | lin | 15549.229 | 1.00× | validated |
| join_filter | sqlite | 24710.021 | 3.27× | validated |
| join_filter | duckdb | 13700.000 | 1.81× | validated |
| join_filter | postgres | 26118.938 | 3.46× | validated |
| join_filter | mysql | 347669.812 | 46.01× | validated |
| join_filter | mongo | 583477.479 | 77.22× | validated |
| join_filter | pandas | 18696.750 | 2.47× | validated |
| join_filter | lin | 7556.020 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
