# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 6460.667 | 1.00× | validated |
| filter_eq | sqlite | 941.446 | 0.15× | validated |
| filter_eq | duckdb | 419.606 | 0.06× | validated |
| filter_eq | pandas | 2213.396 | 0.34× | validated |
| text_substr | lin | 6650.105 | 1.00× | validated |
| text_substr | sqlite | 3901.188 | 0.59× | validated |
| text_substr | duckdb | 464.754 | 0.07× | validated |
| text_substr | pandas | 4406.646 | 0.66× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
