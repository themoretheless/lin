# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| filter_eq | lin | 0.858 | 1.00× | validated |
| filter_eq | sqlite | 688.187 | 802.17× | validated |
| filter_eq | duckdb | 424.250 | 494.52× | validated |
| filter_eq | pandas | 2333.021 | 2719.44× | validated |
| text_substr | lin | 204.794 | 1.00× | validated |
| text_substr | sqlite | 3314.459 | 16.18× | validated |
| text_substr | duckdb | 488.177 | 2.38× | validated |
| text_substr | pandas | 4476.688 | 21.86× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
