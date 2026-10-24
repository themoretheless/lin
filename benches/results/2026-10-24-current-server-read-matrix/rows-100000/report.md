# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.114 | 1.00× | validated |
| point_get | sqlite | 0.994 | 8.72× | validated |
| point_get | duckdb | 39.822 | 349.15× | validated |
| point_get | postgres | 249.988 | 2191.83× | validated |
| point_get | mysql | 327.338 | 2870.02× | validated |
| point_get | mongo | 403.242 | 3535.54× | validated |
| point_get | pandas | 4.468 | 39.18× | validated |
| filter_eq | lin | 0.847 | 1.00× | validated |
| filter_eq | sqlite | 687.610 | 811.91× | validated |
| filter_eq | duckdb | 419.087 | 494.85× | validated |
| filter_eq | postgres | 2284.177 | 2697.09× | validated |
| filter_eq | mysql | 3318.438 | 3918.32× | validated |
| filter_eq | mongo | 4957.833 | 5854.07× | validated |
| filter_eq | pandas | 2295.615 | 2710.60× | validated |
| text_substr | lin | 210.702 | 1.00× | validated |
| text_substr | sqlite | 3484.958 | 16.54× | validated |
| text_substr | duckdb | 453.925 | 2.15× | validated |
| text_substr | postgres | 5066.938 | 24.05× | validated |
| text_substr | mysql | 9433.416 | 44.77× | validated |
| text_substr | mongo | 26317.730 | 124.90× | validated |
| text_substr | pandas | 4618.042 | 21.92× | validated |
| materialize | lin | 5860.438 | 1.00× | validated |
| materialize | sqlite | 20569.291 | 3.51× | validated |
| materialize | duckdb | 12207.959 | 2.08× | validated |
| materialize | postgres | 14636.416 | 2.50× | validated |
| materialize | mysql | 83190.688 | 14.20× | validated |
| materialize | mongo | 41798.770 | 7.13× | validated |
| materialize | pandas | 18245.521 | 3.11× | validated |
| join_inner | lin | 16286.000 | 1.00× | validated |
| join_inner | sqlite | 52062.395 | 3.20× | validated |
| join_inner | duckdb | 28373.980 | 1.74× | validated |
| join_inner | postgres | 39464.021 | 2.42× | validated |
| join_inner | mysql | 166505.438 | 10.22× | validated |
| join_inner | mongo | 1120137.604 | 68.78× | validated |
| join_inner | pandas | 37419.104 | 2.30× | validated |
| join_filter | lin | 7897.229 | 1.00× | validated |
| join_filter | sqlite | 26087.625 | 3.30× | validated |
| join_filter | duckdb | 13436.791 | 1.70× | validated |
| join_filter | postgres | 21037.354 | 2.66× | validated |
| join_filter | mysql | 90613.250 | 11.47× | validated |
| join_filter | mongo | 577422.833 | 73.12× | validated |
| join_filter | pandas | 19349.188 | 2.45× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
