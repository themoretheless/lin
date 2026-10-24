# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 250.514 | 1.00× | validated |
| filter_eq | sqlite | 68.573 | 0.27× | validated |
| filter_eq | duckdb | 186.142 | 0.74× | validated |
| filter_eq | pandas | 256.502 | 1.02× | validated |
| text_substr | lin | 299.479 | 1.00× | validated |
| text_substr | sqlite | 328.186 | 1.10× | validated |
| text_substr | duckdb | 100.774 | 0.34× | validated |
| text_substr | pandas | 479.529 | 1.60× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
