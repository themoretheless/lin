# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.855 | 1.00× | validated |
| filter_eq | sqlite | 691.277 | 808.81× | validated |
| filter_eq | duckdb | 435.908 | 510.02× | validated |
| filter_eq | pandas | 2196.802 | 2570.31× | validated |
| text_substr | lin | 213.890 | 1.00× | validated |
| text_substr | sqlite | 3488.834 | 16.31× | validated |
| text_substr | duckdb | 474.812 | 2.22× | validated |
| text_substr | pandas | 4460.730 | 20.86× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
