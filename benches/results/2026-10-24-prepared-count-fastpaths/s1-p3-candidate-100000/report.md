# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.844 | 1.00× | validated |
| filter_eq | sqlite | 673.104 | 797.40× | validated |
| filter_eq | duckdb | 425.125 | 503.63× | validated |
| filter_eq | pandas | 2197.042 | 2602.74× | validated |
| text_substr | lin | 210.952 | 1.00× | validated |
| text_substr | sqlite | 3246.104 | 15.39× | validated |
| text_substr | duckdb | 465.081 | 2.20× | validated |
| text_substr | pandas | 4393.771 | 20.83× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
