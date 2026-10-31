# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 0.956 | 8.92× | validated |
| point_get | duckdb | 111.265 | 1038.22× | validated |
| point_get | lin | 0.107 | 1.00× | validated |
| filter_eq | sqlite | 69.766 | 116.68× | validated |
| filter_eq | duckdb | 268.163 | 448.50× | validated |
| filter_eq | lin | 0.598 | 1.00× | validated |
| text_substr | sqlite | 325.103 | 18.22× | validated |
| text_substr | duckdb | 248.216 | 13.91× | validated |
| text_substr | lin | 17.842 | 1.00× | validated |
| materialize | sqlite | 1854.573 | 4.74× | validated |
| materialize | duckdb | 1543.417 | 3.94× | validated |
| materialize | lin | 391.460 | 1.00× | validated |
| join_inner | sqlite | 4606.854 | 3.44× | validated |
| join_inner | duckdb | 3275.562 | 2.45× | validated |
| join_inner | lin | 1338.299 | 1.00× | validated |
| join_filter | sqlite | 2340.542 | 3.48× | validated |
| join_filter | duckdb | 1986.844 | 2.96× | validated |
| join_filter | lin | 672.253 | 1.00× | validated |

Versions: {"duckdb": "2.0.0", "lin": "0.4.0", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
