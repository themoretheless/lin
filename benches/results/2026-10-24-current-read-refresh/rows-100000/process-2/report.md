# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | sqlite | 1.012 | 7.55× | validated |
| point_get | duckdb | 42.288 | 315.25× | validated |
| point_get | pandas | 4.646 | 34.64× | validated |
| point_get | lin | 0.134 | 1.00× | validated |
| filter_eq | sqlite | 713.174 | 0.12× | validated |
| filter_eq | duckdb | 451.083 | 0.08× | validated |
| filter_eq | pandas | 2347.636 | 0.39× | validated |
| filter_eq | lin | 6004.188 | 1.00× | validated |
| text_substr | sqlite | 3383.312 | 0.54× | validated |
| text_substr | duckdb | 487.109 | 0.08× | validated |
| text_substr | pandas | 4815.812 | 0.77× | validated |
| text_substr | lin | 6234.167 | 1.00× | validated |
| materialize | sqlite | 21413.125 | 3.28× | validated |
| materialize | duckdb | 12263.834 | 1.88× | validated |
| materialize | pandas | 18344.688 | 2.81× | validated |
| materialize | lin | 6531.437 | 1.00× | validated |
| join_inner | sqlite | 54094.584 | 3.20× | validated |
| join_inner | duckdb | 28719.583 | 1.70× | validated |
| join_inner | pandas | 44762.751 | 2.65× | validated |
| join_inner | lin | 16881.500 | 1.00× | validated |
| join_filter | sqlite | 26761.895 | 2.98× | validated |
| join_filter | duckdb | 14500.833 | 1.61× | validated |
| join_filter | pandas | 21568.312 | 2.40× | validated |
| join_filter | lin | 8994.959 | 1.00× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "pandas": "3.0.6", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
