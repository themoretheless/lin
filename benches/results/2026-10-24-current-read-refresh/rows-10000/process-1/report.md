# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.119 | 1.00× | validated |
| point_get | sqlite | 0.969 | 8.15× | validated |
| point_get | duckdb | 35.003 | 294.19× | validated |
| point_get | pandas | 4.512 | 37.92× | validated |
| filter_eq | lin | 270.252 | 1.00× | validated |
| filter_eq | sqlite | 75.156 | 0.28× | validated |
| filter_eq | duckdb | 190.954 | 0.71× | validated |
| filter_eq | pandas | 276.972 | 1.02× | validated |
| text_substr | lin | 302.638 | 1.00× | validated |
| text_substr | sqlite | 327.424 | 1.08× | validated |
| text_substr | duckdb | 103.197 | 0.34× | validated |
| text_substr | pandas | 497.994 | 1.65× | validated |
| materialize | lin | 371.591 | 1.00× | validated |
| materialize | sqlite | 1891.344 | 5.09× | validated |
| materialize | duckdb | 1314.781 | 3.54× | validated |
| materialize | pandas | 2028.989 | 5.46× | validated |
| join_inner | lin | 1376.055 | 1.00× | validated |
| join_inner | sqlite | 4518.770 | 3.28× | validated |
| join_inner | duckdb | 2745.771 | 2.00× | validated |
| join_inner | pandas | 4214.229 | 3.06× | validated |
| join_filter | lin | 655.979 | 1.00× | validated |
| join_filter | sqlite | 2333.490 | 3.56× | validated |
| join_filter | duckdb | 1501.021 | 2.29× | validated |
| join_filter | pandas | 2463.708 | 3.76× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
