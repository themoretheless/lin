# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 269.566 | 1.00× | validated |
| filter_eq | sqlite | 68.878 | 0.26× | validated |
| filter_eq | duckdb | 200.270 | 0.74× | validated |
| filter_eq | pandas | 275.252 | 1.02× | validated |
| text_substr | lin | 300.512 | 1.00× | validated |
| text_substr | sqlite | 337.926 | 1.12× | validated |
| text_substr | duckdb | 110.118 | 0.37× | validated |
| text_substr | pandas | 499.310 | 1.66× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
