# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 6247.542 | 1.00× | validated |
| filter_eq | sqlite | 670.238 | 0.11× | validated |
| filter_eq | duckdb | 455.096 | 0.07× | validated |
| filter_eq | pandas | 2188.833 | 0.35× | validated |
| text_substr | lin | 6273.938 | 1.00× | validated |
| text_substr | sqlite | 3302.479 | 0.53× | validated |
| text_substr | duckdb | 472.619 | 0.08× | validated |
| text_substr | pandas | 4478.896 | 0.71× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
