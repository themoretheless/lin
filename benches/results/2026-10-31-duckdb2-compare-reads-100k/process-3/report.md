# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 143.168 | 1389.59× | validated |
| point_get | lin | 0.103 | 1.00× | validated |
| point_get | sqlite | 1.008 | 9.79× | validated |
| filter_eq | duckdb | 419.318 | 539.85× | validated |
| filter_eq | lin | 0.777 | 1.00× | validated |
| filter_eq | sqlite | 690.363 | 888.80× | validated |
| text_substr | duckdb | 1677.208 | 7.81× | validated |
| text_substr | lin | 214.793 | 1.00× | validated |
| text_substr | sqlite | 3706.708 | 17.26× | validated |
| materialize | duckdb | 15397.500 | 1.99× | validated |
| materialize | lin | 7746.542 | 1.00× | validated |
| materialize | sqlite | 22048.791 | 2.85× | validated |
| join_inner | duckdb | 41171.250 | 2.43× | validated |
| join_inner | lin | 16927.500 | 1.00× | validated |
| join_inner | sqlite | 226273.375 | 13.37× | validated |
| join_filter | duckdb | 18176.750 | 2.06× | validated |
| join_filter | lin | 8806.833 | 1.00× | validated |
| join_filter | sqlite | 136846.416 | 15.54× | validated |

Versions: {"duckdb": "2.0.0", "lin": "0.4.0", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
