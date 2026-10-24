# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 251.679 | 1.00× | validated |
| filter_eq | sqlite | 68.643 | 0.27× | validated |
| filter_eq | duckdb | 188.896 | 0.75× | validated |
| filter_eq | pandas | 260.651 | 1.04× | validated |
| text_substr | lin | 275.662 | 1.00× | validated |
| text_substr | sqlite | 337.003 | 1.22× | validated |
| text_substr | duckdb | 102.407 | 0.37× | validated |
| text_substr | pandas | 484.630 | 1.76× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
