# Validated read API benchmark

Rows: 10000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.116 | 1.00× | validated |
| point_get | sqlite | 0.969 | 8.36× | validated |
| point_get | duckdb | 37.406 | 322.73× | validated |
| point_get | pandas | 4.497 | 38.80× | validated |
| filter_eq | lin | 260.064 | 1.00× | validated |
| filter_eq | sqlite | 69.951 | 0.27× | validated |
| filter_eq | duckdb | 190.983 | 0.73× | validated |
| filter_eq | pandas | 262.930 | 1.01× | validated |
| text_substr | lin | 294.321 | 1.00× | validated |
| text_substr | sqlite | 327.424 | 1.11× | validated |
| text_substr | duckdb | 103.197 | 0.35× | validated |
| text_substr | pandas | 484.745 | 1.65× | validated |
| materialize | lin | 369.803 | 1.00× | validated |
| materialize | sqlite | 1891.344 | 5.11× | validated |
| materialize | duckdb | 1259.411 | 3.41× | validated |
| materialize | pandas | 1854.063 | 5.01× | validated |
| join_inner | lin | 1347.903 | 1.00× | validated |
| join_inner | sqlite | 4518.770 | 3.35× | validated |
| join_inner | duckdb | 2675.938 | 1.99× | validated |
| join_inner | pandas | 4028.667 | 2.99× | validated |
| join_filter | lin | 657.646 | 1.00× | validated |
| join_filter | sqlite | 2337.260 | 3.55× | validated |
| join_filter | duckdb | 1404.660 | 2.14× | validated |
| join_filter | pandas | 2332.468 | 3.55× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
