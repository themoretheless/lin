# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.113 | 1.00× | validated |
| point_get | sqlite | 0.990 | 8.76× | validated |
| point_get | duckdb | 239.708 | 2121.31× | validated |
| filter_eq | lin | 0.778 | 1.00× | validated |
| filter_eq | sqlite | 696.863 | 895.58× | validated |
| filter_eq | duckdb | 629.144 | 808.55× | validated |
| text_substr | lin | 208.973 | 1.00× | validated |
| text_substr | sqlite | 3665.542 | 17.54× | validated |
| text_substr | duckdb | 1664.750 | 7.97× | validated |
| materialize | lin | 7704.958 | 1.00× | validated |
| materialize | sqlite | 22240.334 | 2.89× | validated |
| materialize | duckdb | 14999.291 | 1.95× | validated |
| join_inner | lin | 19204.667 | 1.00× | validated |
| join_inner | sqlite | 54978.041 | 2.86× | validated |
| join_inner | duckdb | 34165.750 | 1.78× | validated |
| join_filter | lin | 10061.500 | 1.00× | validated |
| join_filter | sqlite | 28881.083 | 2.87× | validated |
| join_filter | duckdb | 18148.959 | 1.80× | validated |

Versions: {"duckdb": "2.0.0", "lin": "0.4.0", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
