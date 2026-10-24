# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.856 | 1.00× | validated |
| filter_eq | sqlite | 682.545 | 797.15× | validated |
| filter_eq | duckdb | 439.875 | 513.74× | validated |
| filter_eq | pandas | 2243.615 | 2620.35× | validated |
| text_substr | lin | 208.757 | 1.00× | validated |
| text_substr | sqlite | 3355.938 | 16.08× | validated |
| text_substr | duckdb | 486.965 | 2.33× | validated |
| text_substr | pandas | 4665.021 | 22.35× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
