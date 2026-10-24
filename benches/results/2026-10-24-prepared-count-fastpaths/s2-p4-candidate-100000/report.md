# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.883 | 1.00× | validated |
| filter_eq | sqlite | 681.934 | 772.37× | validated |
| filter_eq | duckdb | 500.606 | 567.00× | validated |
| filter_eq | pandas | 2405.291 | 2724.29× | validated |
| text_substr | lin | 211.003 | 1.00× | validated |
| text_substr | sqlite | 3433.041 | 16.27× | validated |
| text_substr | duckdb | 503.616 | 2.39× | validated |
| text_substr | pandas | 5303.583 | 25.14× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
