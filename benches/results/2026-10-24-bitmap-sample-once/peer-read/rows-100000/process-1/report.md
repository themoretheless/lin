# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.117 | 1.00× | validated |
| point_get | sqlite | 0.967 | 8.25× | validated |
| point_get | duckdb | 37.350 | 318.41× | validated |
| point_get | postgres | 259.932 | 2215.92× | validated |
| point_get | mysql | 327.830 | 2794.75× | validated |
| point_get | mongo | 2082.938 | 17757.04× | validated |
| point_get | pandas | 4.482 | 38.21× | validated |
| filter_eq | lin | 0.858 | 1.00× | validated |
| filter_eq | sqlite | 673.735 | 785.11× | validated |
| filter_eq | duckdb | 452.002 | 526.72× | validated |
| filter_eq | postgres | 2685.583 | 3129.52× | validated |
| filter_eq | mysql | 3489.771 | 4066.65× | validated |
| filter_eq | mongo | 13571.063 | 15814.43× | validated |
| filter_eq | pandas | 2147.719 | 2502.75× | validated |
| text_substr | lin | 210.976 | 1.00× | validated |
| text_substr | sqlite | 3265.146 | 15.48× | validated |
| text_substr | duckdb | 476.690 | 2.26× | validated |
| text_substr | postgres | 5559.250 | 26.35× | validated |
| text_substr | mysql | 9657.999 | 45.78× | validated |
| text_substr | mongo | 39867.438 | 188.97× | validated |
| text_substr | pandas | 4391.562 | 20.82× | validated |
| materialize | lin | 4974.146 | 1.00× | validated |
| materialize | sqlite | 19671.000 | 3.95× | validated |
| materialize | duckdb | 12563.604 | 2.53× | validated |
| materialize | postgres | 14552.438 | 2.93× | validated |
| materialize | mysql | 83701.520 | 16.83× | validated |
| materialize | mongo | 69286.812 | 13.93× | validated |
| materialize | pandas | 16491.312 | 3.32× | validated |
| join_inner | lin | 16483.770 | 1.00× | validated |
| join_inner | sqlite | 49599.146 | 3.01× | validated |
| join_inner | duckdb | 29495.041 | 1.79× | validated |
| join_inner | postgres | 47803.959 | 2.90× | validated |
| join_inner | mysql | 166367.688 | 10.09× | validated |
| join_inner | mongo | 1507018.084 | 91.42× | validated |
| join_inner | pandas | 36156.500 | 2.19× | validated |
| join_filter | lin | 7737.979 | 1.00× | validated |
| join_filter | sqlite | 24681.500 | 3.19× | validated |
| join_filter | duckdb | 14704.604 | 1.90× | validated |
| join_filter | postgres | 21872.062 | 2.83× | validated |
| join_filter | mysql | 123676.271 | 15.98× | validated |
| join_filter | mongo | 565328.854 | 73.06× | validated |
| join_filter | pandas | 18280.812 | 2.36× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
