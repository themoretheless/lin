# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.653 | 1.00× | validated |
| filter_eq | sqlite | 7.328 | 11.23× | validated |
| filter_eq | duckdb | 143.921 | 220.50× | validated |
| filter_eq | pandas | 58.240 | 89.23× | validated |
| text_substr | lin | 1.781 | 1.00× | validated |
| text_substr | sqlite | 32.534 | 18.27× | validated |
| text_substr | duckdb | 62.183 | 34.92× | validated |
| text_substr | pandas | 74.849 | 42.04× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
