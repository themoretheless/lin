# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.681 | 1.00× | validated |
| filter_eq | sqlite | 69.532 | 102.10× | validated |
| filter_eq | duckdb | 197.240 | 289.64× | validated |
| filter_eq | pandas | 270.287 | 396.90× | validated |
| text_substr | lin | 19.784 | 1.00× | validated |
| text_substr | sqlite | 327.744 | 16.57× | validated |
| text_substr | duckdb | 104.769 | 5.30× | validated |
| text_substr | pandas | 494.240 | 24.98× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
