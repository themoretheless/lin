# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| text_substr | lin | 723.844 | 1.00× | validated |
| text_substr | duckdb | 569.174 | 0.79× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0"}

Missing/failed peers are never counted as wins.
