# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 13499.708 | 1.00× | validated |
| filter_eq | sqlite | 1727.833 | 0.13× | validated |
| filter_eq | duckdb | 445.767 | 0.03× | validated |
| filter_eq | pandas | 2193.417 | 0.16× | validated |
| text_substr | lin | 16769.250 | 1.00× | validated |
| text_substr | sqlite | 7994.188 | 0.48× | validated |
| text_substr | duckdb | 481.181 | 0.03× | validated |
| text_substr | pandas | 4393.959 | 0.26× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
