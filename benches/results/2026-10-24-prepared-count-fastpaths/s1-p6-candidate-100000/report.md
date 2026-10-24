# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.838 | 1.00× | validated |
| filter_eq | sqlite | 674.372 | 804.30× | validated |
| filter_eq | duckdb | 423.885 | 505.55× | validated |
| filter_eq | pandas | 2316.968 | 2763.37× | validated |
| text_substr | lin | 208.484 | 1.00× | validated |
| text_substr | sqlite | 3366.562 | 16.15× | validated |
| text_substr | duckdb | 477.477 | 2.29× | validated |
| text_substr | pandas | 4862.251 | 23.32× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
