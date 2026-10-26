# Validated read API benchmark

Rows: 1000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.115 | 1.00× | validated |
| point_get | sqlite | 0.936 | 8.14× | validated |
| point_get | duckdb | 41.091 | 357.41× | validated |
| point_get | postgres | 305.130 | 2654.03× | validated |
| point_get | mysql | 325.796 | 2833.78× | validated |
| point_get | mongo | 458.597 | 3988.89× | validated |
| point_get | pandas | 4.456 | 38.75× | validated |
| filter_eq | lin | 0.656 | 1.00× | validated |
| filter_eq | sqlite | 7.490 | 11.42× | validated |
| filter_eq | duckdb | 142.852 | 217.88× | validated |
| filter_eq | postgres | 322.208 | 491.44× | validated |
| filter_eq | mysql | 377.119 | 575.19× | validated |
| filter_eq | mongo | 559.440 | 853.27× | validated |
| filter_eq | pandas | 57.965 | 88.41× | validated |
| text_substr | lin | 1.804 | 1.00× | validated |
| text_substr | sqlite | 32.594 | 18.07× | validated |
| text_substr | duckdb | 63.675 | 35.29× | validated |
| text_substr | postgres | 362.075 | 200.68× | validated |
| text_substr | mysql | 429.504 | 238.05× | validated |
| text_substr | mongo | 821.406 | 455.26× | validated |
| text_substr | pandas | 78.921 | 43.74× | validated |
| materialize | lin | 38.870 | 1.00× | validated |
| materialize | sqlite | 169.495 | 4.36× | validated |
| materialize | duckdb | 299.436 | 7.70× | validated |
| materialize | postgres | 447.113 | 11.50× | validated |
| materialize | mysql | 1262.035 | 32.47× | validated |
| materialize | mongo | 1398.472 | 35.98× | validated |
| materialize | pandas | 401.701 | 10.33× | validated |
| join_inner | lin | 134.047 | 1.00× | validated |
| join_inner | sqlite | 398.714 | 2.97× | validated |
| join_inner | duckdb | 371.827 | 2.77× | validated |
| join_inner | postgres | 669.399 | 4.99× | validated |
| join_inner | mysql | 2359.438 | 17.60× | validated |
| join_inner | mongo | 13201.979 | 98.49× | validated |
| join_inner | pandas | 779.212 | 5.81× | validated |
| join_filter | lin | 67.510 | 1.00× | validated |
| join_filter | sqlite | 205.783 | 3.05× | validated |
| join_filter | duckdb | 264.256 | 3.91× | validated |
| join_filter | postgres | 510.729 | 7.57× | validated |
| join_filter | mysql | 1434.139 | 21.24× | validated |
| join_filter | mongo | 7747.604 | 114.76× | validated |
| join_filter | pandas | 703.057 | 10.41× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
