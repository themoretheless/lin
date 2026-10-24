# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 281.583 | 1.00× | validated |
| filter_eq | sqlite | 69.666 | 0.25× | validated |
| filter_eq | duckdb | 188.204 | 0.67× | validated |
| filter_eq | pandas | 251.241 | 0.89× | validated |
| text_substr | lin | 303.258 | 1.00× | validated |
| text_substr | sqlite | 337.225 | 1.11× | validated |
| text_substr | duckdb | 101.512 | 0.33× | validated |
| text_substr | pandas | 467.881 | 1.54× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
