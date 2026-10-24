# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 26.391 | 1.00× | validated |
| filter_eq | sqlite | 7.552 | 0.29× | validated |
| filter_eq | duckdb | 130.660 | 4.95× | validated |
| filter_eq | pandas | 56.717 | 2.15× | validated |
| text_substr | lin | 26.618 | 1.00× | validated |
| text_substr | sqlite | 32.368 | 1.22× | validated |
| text_substr | duckdb | 62.493 | 2.35× | validated |
| text_substr | pandas | 76.412 | 2.87× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
