# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 271.228 | 1.00× | validated |
| filter_eq | sqlite | 73.509 | 0.27× | validated |
| filter_eq | duckdb | 187.927 | 0.69× | validated |
| filter_eq | pandas | 267.915 | 0.99× | validated |
| text_substr | lin | 298.615 | 1.00× | validated |
| text_substr | sqlite | 338.935 | 1.14× | validated |
| text_substr | duckdb | 104.550 | 0.35× | validated |
| text_substr | pandas | 492.765 | 1.65× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
