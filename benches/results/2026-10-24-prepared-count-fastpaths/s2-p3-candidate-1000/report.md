# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.662 | 1.00× | validated |
| filter_eq | sqlite | 7.533 | 11.38× | validated |
| filter_eq | duckdb | 138.867 | 209.71× | validated |
| filter_eq | pandas | 59.001 | 89.10× | validated |
| text_substr | lin | 1.757 | 1.00× | validated |
| text_substr | sqlite | 31.253 | 17.78× | validated |
| text_substr | duckdb | 65.506 | 37.27× | validated |
| text_substr | pandas | 76.218 | 43.37× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
