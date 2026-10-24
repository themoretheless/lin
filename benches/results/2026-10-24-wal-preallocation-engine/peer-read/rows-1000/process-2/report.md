# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.880 | 7.93× | validated |
| point_get | duckdb | 37.373 | 336.53× | validated |
| point_get | postgres | 305.618 | 2752.02× | validated |
| point_get | mysql | 317.295 | 2857.17× | validated |
| point_get | mongo | 402.710 | 3626.32× | validated |
| point_get | pandas | 4.359 | 39.25× | validated |
| point_get | lin | 0.111 | 1.00× | validated |
| filter_eq | sqlite | 7.703 | 12.14× | validated |
| filter_eq | duckdb | 138.211 | 217.78× | validated |
| filter_eq | postgres | 332.971 | 524.65× | validated |
| filter_eq | mysql | 346.658 | 546.22× | validated |
| filter_eq | mongo | 484.662 | 763.67× | validated |
| filter_eq | pandas | 56.132 | 88.44× | validated |
| filter_eq | lin | 0.635 | 1.00× | validated |
| text_substr | sqlite | 31.097 | 16.79× | validated |
| text_substr | duckdb | 65.066 | 35.13× | validated |
| text_substr | postgres | 347.681 | 187.71× | validated |
| text_substr | mysql | 397.861 | 214.80× | validated |
| text_substr | mongo | 695.632 | 375.57× | validated |
| text_substr | pandas | 73.818 | 39.85× | validated |
| text_substr | lin | 1.852 | 1.00× | validated |
| materialize | sqlite | 165.203 | 4.35× | validated |
| materialize | duckdb | 296.351 | 7.80× | validated |
| materialize | postgres | 431.546 | 11.36× | validated |
| materialize | mysql | 1217.021 | 32.04× | validated |
| materialize | mongo | 1239.422 | 32.63× | validated |
| materialize | pandas | 370.330 | 9.75× | validated |
| materialize | lin | 37.986 | 1.00× | validated |
| join_inner | sqlite | 388.573 | 2.97× | validated |
| join_inner | duckdb | 353.432 | 2.71× | validated |
| join_inner | postgres | 646.326 | 4.95× | validated |
| join_inner | mysql | 2230.031 | 17.07× | validated |
| join_inner | mongo | 12633.959 | 96.71× | validated |
| join_inner | pandas | 712.767 | 5.46× | validated |
| join_inner | lin | 130.635 | 1.00× | validated |
| join_filter | sqlite | 206.234 | 3.15× | validated |
| join_filter | duckdb | 245.200 | 3.75× | validated |
| join_filter | postgres | 495.599 | 7.57× | validated |
| join_filter | mysql | 1334.688 | 20.39× | validated |
| join_filter | mongo | 6674.917 | 101.97× | validated |
| join_filter | pandas | 625.411 | 9.55× | validated |
| join_filter | lin | 65.457 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
