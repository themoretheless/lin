# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 33.300 | 304.98× | validated |
| point_get | postgres | 240.375 | 2201.45× | validated |
| point_get | mysql | 344.743 | 3157.29× | validated |
| point_get | mongo | 501.392 | 4591.94× | validated |
| point_get | pandas | 4.465 | 40.89× | validated |
| point_get | lin | 0.109 | 1.00× | validated |
| point_get | sqlite | 0.962 | 8.81× | validated |
| filter_eq | duckdb | 432.581 | 535.60× | validated |
| filter_eq | postgres | 2259.948 | 2798.15× | validated |
| filter_eq | mysql | 3504.604 | 4339.22× | validated |
| filter_eq | mongo | 4911.688 | 6081.39× | validated |
| filter_eq | pandas | 2118.104 | 2622.53× | validated |
| filter_eq | lin | 0.808 | 1.00× | validated |
| filter_eq | sqlite | 671.256 | 831.11× | validated |
| text_substr | duckdb | 464.863 | 2.31× | validated |
| text_substr | postgres | 5096.917 | 25.28× | validated |
| text_substr | mysql | 9346.354 | 46.36× | validated |
| text_substr | mongo | 26430.520 | 131.11× | validated |
| text_substr | pandas | 4483.333 | 22.24× | validated |
| text_substr | lin | 201.587 | 1.00× | validated |
| text_substr | sqlite | 3361.126 | 16.67× | validated |
| materialize | duckdb | 12593.229 | 2.45× | validated |
| materialize | postgres | 14109.041 | 2.74× | validated |
| materialize | mysql | 82113.188 | 15.95× | validated |
| materialize | mongo | 42446.584 | 8.25× | validated |
| materialize | pandas | 16850.458 | 3.27× | validated |
| materialize | lin | 5147.499 | 1.00× | validated |
| materialize | sqlite | 20021.771 | 3.89× | validated |
| join_inner | duckdb | 29456.083 | 1.88× | validated |
| join_inner | postgres | 46350.104 | 2.97× | validated |
| join_inner | mysql | 167673.271 | 10.73× | validated |
| join_inner | mongo | 1103342.667 | 70.59× | validated |
| join_inner | pandas | 36069.021 | 2.31× | validated |
| join_inner | lin | 15630.292 | 1.00× | validated |
| join_inner | sqlite | 50363.541 | 3.22× | validated |
| join_filter | duckdb | 14070.000 | 1.99× | validated |
| join_filter | postgres | 20669.375 | 2.92× | validated |
| join_filter | mysql | 89622.875 | 12.68× | validated |
| join_filter | mongo | 567865.542 | 80.32× | validated |
| join_filter | pandas | 18106.937 | 2.56× | validated |
| join_filter | lin | 7069.646 | 1.00× | validated |
| join_filter | sqlite | 25107.334 | 3.55× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
