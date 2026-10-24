# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.667 | 1.00× | validated |
| filter_eq | sqlite | 69.594 | 104.34× | validated |
| filter_eq | duckdb | 195.413 | 292.98× | validated |
| filter_eq | pandas | 262.563 | 393.65× | validated |
| text_substr | lin | 16.086 | 1.00× | validated |
| text_substr | sqlite | 332.484 | 20.67× | validated |
| text_substr | duckdb | 113.012 | 7.03× | validated |
| text_substr | pandas | 490.427 | 30.49× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
