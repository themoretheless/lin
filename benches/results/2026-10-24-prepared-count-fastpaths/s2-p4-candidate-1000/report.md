# Validated read API benchmark

Rows: 1000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.696 | 1.00× | validated |
| filter_eq | sqlite | 7.649 | 10.99× | validated |
| filter_eq | duckdb | 129.406 | 185.94× | validated |
| filter_eq | pandas | 59.835 | 85.98× | validated |
| text_substr | lin | 1.891 | 1.00× | validated |
| text_substr | sqlite | 33.148 | 17.53× | validated |
| text_substr | duckdb | 57.205 | 30.25× | validated |
| text_substr | pandas | 77.947 | 41.22× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
