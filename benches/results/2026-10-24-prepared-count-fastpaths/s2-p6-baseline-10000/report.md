# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 269.971 | 1.00× | validated |
| filter_eq | sqlite | 67.330 | 0.25× | validated |
| filter_eq | duckdb | 202.645 | 0.75× | validated |
| filter_eq | pandas | 263.646 | 0.98× | validated |
| text_substr | lin | 281.736 | 1.00× | validated |
| text_substr | sqlite | 332.619 | 1.18× | validated |
| text_substr | duckdb | 114.152 | 0.41× | validated |
| text_substr | pandas | 487.505 | 1.73× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
