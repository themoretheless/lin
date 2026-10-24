# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.893 | 7.74× | validated |
| point_get | duckdb | 40.129 | 347.71× | validated |
| point_get | pandas | 4.392 | 38.05× | validated |
| point_get | lin | 0.115 | 1.00× | validated |
| filter_eq | sqlite | 66.503 | 88.87× | validated |
| filter_eq | duckdb | 185.892 | 248.42× | validated |
| filter_eq | pandas | 265.631 | 354.98× | validated |
| filter_eq | lin | 0.748 | 1.00× | validated |
| text_substr | sqlite | 322.746 | 18.73× | validated |
| text_substr | duckdb | 104.532 | 6.07× | validated |
| text_substr | pandas | 491.370 | 28.52× | validated |
| text_substr | lin | 17.227 | 1.00× | validated |
| materialize | sqlite | 1828.406 | 4.88× | validated |
| materialize | duckdb | 1235.055 | 3.30× | validated |
| materialize | pandas | 1805.364 | 4.82× | validated |
| materialize | lin | 374.673 | 1.00× | validated |
| join_inner | sqlite | 4394.458 | 3.22× | validated |
| join_inner | duckdb | 2538.604 | 1.86× | validated |
| join_inner | pandas | 3823.001 | 2.80× | validated |
| join_inner | lin | 1364.403 | 1.00× | validated |
| join_filter | sqlite | 2334.146 | 3.38× | validated |
| join_filter | duckdb | 1385.292 | 2.00× | validated |
| join_filter | pandas | 2208.135 | 3.19× | validated |
| join_filter | lin | 691.426 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
