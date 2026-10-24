# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 26.918 | 1.00× | validated |
| filter_eq | sqlite | 8.220 | 0.31× | validated |
| filter_eq | duckdb | 199.599 | 7.41× | validated |
| filter_eq | pandas | 59.824 | 2.22× | validated |
| text_substr | lin | 29.445 | 1.00× | validated |
| text_substr | sqlite | 33.571 | 1.14× | validated |
| text_substr | duckdb | 91.828 | 3.12× | validated |
| text_substr | pandas | 78.952 | 2.68× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
