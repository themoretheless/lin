# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.668 | 1.00× | validated |
| filter_eq | sqlite | 68.770 | 102.93× | validated |
| filter_eq | duckdb | 183.116 | 274.08× | validated |
| filter_eq | pandas | 262.198 | 392.45× | validated |
| text_substr | lin | 19.826 | 1.00× | validated |
| text_substr | sqlite | 319.046 | 16.09× | validated |
| text_substr | duckdb | 98.706 | 4.98× | validated |
| text_substr | pandas | 478.823 | 24.15× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
