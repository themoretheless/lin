# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 40.038 | 351.04× | validated |
| point_get | postgres | 239.825 | 2102.73× | validated |
| point_get | mysql | 317.392 | 2782.82× | validated |
| point_get | mongo | 403.242 | 3535.54× | validated |
| point_get | pandas | 4.600 | 40.34× | validated |
| point_get | lin | 0.114 | 1.00× | validated |
| point_get | sqlite | 0.969 | 8.49× | validated |
| filter_eq | duckdb | 419.087 | 494.85× | validated |
| filter_eq | postgres | 2238.490 | 2643.15× | validated |
| filter_eq | mysql | 3336.166 | 3939.25× | validated |
| filter_eq | mongo | 4943.562 | 5837.22× | validated |
| filter_eq | pandas | 2295.615 | 2710.60× | validated |
| filter_eq | lin | 0.847 | 1.00× | validated |
| filter_eq | sqlite | 687.610 | 811.91× | validated |
| text_substr | duckdb | 447.337 | 2.14× | validated |
| text_substr | postgres | 5051.708 | 24.19× | validated |
| text_substr | mysql | 9386.771 | 44.94× | validated |
| text_substr | mongo | 26701.042 | 127.84× | validated |
| text_substr | pandas | 4618.042 | 22.11× | validated |
| text_substr | lin | 208.859 | 1.00× | validated |
| text_substr | sqlite | 3524.021 | 16.87× | validated |
| materialize | duckdb | 12324.791 | 1.94× | validated |
| materialize | postgres | 14726.833 | 2.32× | validated |
| materialize | mysql | 83121.938 | 13.08× | validated |
| materialize | mongo | 42466.938 | 6.68× | validated |
| materialize | pandas | 18330.709 | 2.89× | validated |
| materialize | lin | 6353.751 | 1.00× | validated |
| materialize | sqlite | 21791.334 | 3.43× | validated |
| join_inner | duckdb | 27525.000 | 1.69× | validated |
| join_inner | postgres | 47096.750 | 2.89× | validated |
| join_inner | mysql | 170472.729 | 10.47× | validated |
| join_inner | mongo | 1120137.604 | 68.78× | validated |
| join_inner | pandas | 37746.354 | 2.32× | validated |
| join_inner | lin | 16286.000 | 1.00× | validated |
| join_inner | sqlite | 72066.750 | 4.43× | validated |
| join_filter | duckdb | 13436.791 | 1.70× | validated |
| join_filter | postgres | 20866.812 | 2.64× | validated |
| join_filter | mysql | 91679.333 | 11.61× | validated |
| join_filter | mongo | 576729.125 | 73.03× | validated |
| join_filter | pandas | 19625.146 | 2.49× | validated |
| join_filter | lin | 7897.229 | 1.00× | validated |
| join_filter | sqlite | 26527.709 | 3.36× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
