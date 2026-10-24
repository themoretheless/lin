# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 5870.125 | 1.00× | validated |
| filter_eq | sqlite | 695.018 | 0.12× | validated |
| filter_eq | duckdb | 469.859 | 0.08× | validated |
| filter_eq | pandas | 2282.959 | 0.39× | validated |
| text_substr | lin | 6188.875 | 1.00× | validated |
| text_substr | sqlite | 3447.375 | 0.56× | validated |
| text_substr | duckdb | 503.090 | 0.08× | validated |
| text_substr | pandas | 5092.188 | 0.82× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
