# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 271.976 | 1.00× | validated |
| filter_eq | sqlite | 70.659 | 0.26× | validated |
| filter_eq | duckdb | 192.847 | 0.71× | validated |
| filter_eq | pandas | 263.882 | 0.97× | validated |
| text_substr | lin | 314.101 | 1.00× | validated |
| text_substr | sqlite | 334.557 | 1.07× | validated |
| text_substr | duckdb | 103.938 | 0.33× | validated |
| text_substr | pandas | 493.662 | 1.57× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
