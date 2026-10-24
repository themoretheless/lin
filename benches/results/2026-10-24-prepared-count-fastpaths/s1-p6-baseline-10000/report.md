# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 281.558 | 1.00× | validated |
| filter_eq | sqlite | 69.006 | 0.25× | validated |
| filter_eq | duckdb | 178.312 | 0.63× | validated |
| filter_eq | pandas | 267.785 | 0.95× | validated |
| text_substr | lin | 301.095 | 1.00× | validated |
| text_substr | sqlite | 315.399 | 1.05× | validated |
| text_substr | duckdb | 87.376 | 0.29× | validated |
| text_substr | pandas | 483.217 | 1.60× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
