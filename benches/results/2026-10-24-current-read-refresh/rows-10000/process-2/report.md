# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.942 | 8.19× | validated |
| point_get | duckdb | 39.969 | 347.42× | validated |
| point_get | pandas | 4.497 | 39.09× | validated |
| point_get | lin | 0.115 | 1.00× | validated |
| filter_eq | sqlite | 69.951 | 0.27× | validated |
| filter_eq | duckdb | 194.528 | 0.76× | validated |
| filter_eq | pandas | 262.930 | 1.02× | validated |
| filter_eq | lin | 256.581 | 1.00× | validated |
| text_substr | sqlite | 324.689 | 1.16× | validated |
| text_substr | duckdb | 101.037 | 0.36× | validated |
| text_substr | pandas | 484.745 | 1.74× | validated |
| text_substr | lin | 278.703 | 1.00× | validated |
| materialize | sqlite | 1860.688 | 5.10× | validated |
| materialize | duckdb | 1241.660 | 3.40× | validated |
| materialize | pandas | 1820.688 | 4.99× | validated |
| materialize | lin | 364.716 | 1.00× | validated |
| join_inner | sqlite | 4488.730 | 3.33× | validated |
| join_inner | duckdb | 2675.938 | 1.99× | validated |
| join_inner | pandas | 3948.625 | 2.93× | validated |
| join_inner | lin | 1347.903 | 1.00× | validated |
| join_filter | sqlite | 2337.260 | 3.55× | validated |
| join_filter | duckdb | 1404.660 | 2.14× | validated |
| join_filter | pandas | 2332.468 | 3.55× | validated |
| join_filter | lin | 657.646 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
