# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.688 | 1.00× | validated |
| filter_eq | sqlite | 69.506 | 101.01× | validated |
| filter_eq | duckdb | 179.823 | 261.34× | validated |
| filter_eq | pandas | 264.076 | 383.78× | validated |
| text_substr | lin | 16.938 | 1.00× | validated |
| text_substr | sqlite | 324.073 | 19.13× | validated |
| text_substr | duckdb | 92.013 | 5.43× | validated |
| text_substr | pandas | 488.160 | 28.82× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
