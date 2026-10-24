# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 258.416 | 1.00× | validated |
| filter_eq | sqlite | 68.310 | 0.26× | validated |
| filter_eq | duckdb | 190.054 | 0.74× | validated |
| filter_eq | pandas | 274.573 | 1.06× | validated |
| text_substr | lin | 280.443 | 1.00× | validated |
| text_substr | sqlite | 328.542 | 1.17× | validated |
| text_substr | duckdb | 105.406 | 0.38× | validated |
| text_substr | pandas | 480.440 | 1.71× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
