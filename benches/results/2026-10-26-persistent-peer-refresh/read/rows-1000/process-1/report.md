# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.115 | 1.00× | validated |
| point_get | sqlite | 0.918 | 7.98× | validated |
| point_get | duckdb | 34.316 | 298.49× | validated |
| point_get | postgres | 282.881 | 2460.50× | validated |
| point_get | mysql | 325.796 | 2833.78× | validated |
| point_get | mongo | 444.583 | 3866.99× | validated |
| point_get | pandas | 4.387 | 38.16× | validated |
| filter_eq | lin | 0.652 | 1.00× | validated |
| filter_eq | sqlite | 7.490 | 11.48× | validated |
| filter_eq | duckdb | 136.911 | 209.92× | validated |
| filter_eq | postgres | 308.576 | 473.13× | validated |
| filter_eq | mysql | 377.119 | 578.22× | validated |
| filter_eq | mongo | 559.440 | 857.77× | validated |
| filter_eq | pandas | 58.206 | 89.25× | validated |
| text_substr | lin | 1.804 | 1.00× | validated |
| text_substr | sqlite | 31.975 | 17.72× | validated |
| text_substr | duckdb | 61.708 | 34.20× | validated |
| text_substr | postgres | 357.021 | 197.88× | validated |
| text_substr | mysql | 427.170 | 236.76× | validated |
| text_substr | mongo | 821.406 | 455.26× | validated |
| text_substr | pandas | 78.921 | 43.74× | validated |
| materialize | lin | 38.781 | 1.00× | validated |
| materialize | sqlite | 166.020 | 4.28× | validated |
| materialize | duckdb | 294.663 | 7.60× | validated |
| materialize | postgres | 447.113 | 11.53× | validated |
| materialize | mysql | 1259.052 | 32.47× | validated |
| materialize | mongo | 1414.083 | 36.46× | validated |
| materialize | pandas | 401.701 | 10.36× | validated |
| join_inner | lin | 135.009 | 1.00× | validated |
| join_inner | sqlite | 397.972 | 2.95× | validated |
| join_inner | duckdb | 361.636 | 2.68× | validated |
| join_inner | postgres | 669.399 | 4.96× | validated |
| join_inner | mysql | 2359.438 | 17.48× | validated |
| join_inner | mongo | 13441.896 | 99.56× | validated |
| join_inner | pandas | 779.212 | 5.77× | validated |
| join_filter | lin | 66.292 | 1.00× | validated |
| join_filter | sqlite | 205.080 | 3.09× | validated |
| join_filter | duckdb | 251.061 | 3.79× | validated |
| join_filter | postgres | 504.450 | 7.61× | validated |
| join_filter | mysql | 1411.312 | 21.29× | validated |
| join_filter | mongo | 7747.604 | 116.87× | validated |
| join_filter | pandas | 703.057 | 10.61× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
