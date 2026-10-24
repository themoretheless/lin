# Validated read API benchmark

Rows: 10000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 254.870 | 1.00× | validated |
| filter_eq | sqlite | 69.590 | 0.27× | validated |
| filter_eq | duckdb | 188.269 | 0.74× | validated |
| filter_eq | pandas | 337.306 | 1.32× | validated |
| text_substr | lin | 286.139 | 1.00× | validated |
| text_substr | sqlite | 336.085 | 1.17× | validated |
| text_substr | duckdb | 101.548 | 0.35× | validated |
| text_substr | pandas | 760.421 | 2.66× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
