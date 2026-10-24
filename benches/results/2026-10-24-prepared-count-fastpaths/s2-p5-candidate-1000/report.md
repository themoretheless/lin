# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.653 | 1.00× | validated |
| filter_eq | sqlite | 7.383 | 11.31× | validated |
| filter_eq | duckdb | 136.452 | 208.98× | validated |
| filter_eq | pandas | 55.397 | 84.84× | validated |
| text_substr | lin | 1.888 | 1.00× | validated |
| text_substr | sqlite | 32.865 | 17.41× | validated |
| text_substr | duckdb | 55.519 | 29.41× | validated |
| text_substr | pandas | 74.143 | 39.27× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
