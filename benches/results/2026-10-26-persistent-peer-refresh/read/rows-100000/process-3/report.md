# Validated read API benchmark

Rows: 100000; process repetitions: 1; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | duckdb | 40.499 | 357.99× | validated |
| point_get | postgres | 242.459 | 2143.25× | validated |
| point_get | mysql | 328.763 | 2906.14× | validated |
| point_get | mongo | 464.531 | 4106.28× | validated |
| point_get | pandas | 4.540 | 40.13× | validated |
| point_get | lin | 0.113 | 1.00× | validated |
| point_get | sqlite | 1.025 | 9.06× | validated |
| filter_eq | duckdb | 451.968 | 527.35× | validated |
| filter_eq | postgres | 2375.188 | 2771.32× | validated |
| filter_eq | mysql | 3344.458 | 3902.25× | validated |
| filter_eq | mongo | 5317.833 | 6204.74× | validated |
| filter_eq | pandas | 2344.271 | 2735.25× | validated |
| filter_eq | lin | 0.857 | 1.00× | validated |
| filter_eq | sqlite | 685.586 | 799.93× | validated |
| text_substr | duckdb | 495.560 | 2.39× | validated |
| text_substr | postgres | 5240.125 | 25.25× | validated |
| text_substr | mysql | 9753.583 | 47.01× | validated |
| text_substr | mongo | 28091.250 | 135.39× | validated |
| text_substr | pandas | 5171.645 | 24.92× | validated |
| text_substr | lin | 207.490 | 1.00× | validated |
| text_substr | sqlite | 3505.104 | 16.89× | validated |
| materialize | duckdb | 12979.854 | 1.80× | validated |
| materialize | postgres | 18157.250 | 2.52× | validated |
| materialize | mysql | 84388.105 | 11.72× | validated |
| materialize | mongo | 44314.917 | 6.16× | validated |
| materialize | pandas | 19079.313 | 2.65× | validated |
| materialize | lin | 7199.292 | 1.00× | validated |
| materialize | sqlite | 21598.562 | 3.00× | validated |
| join_inner | duckdb | 30482.270 | 1.84× | validated |
| join_inner | postgres | 48391.354 | 2.93× | validated |
| join_inner | mysql | 171818.521 | 10.39× | validated |
| join_inner | mongo | 1162723.375 | 70.33× | validated |
| join_inner | pandas | 38359.229 | 2.32× | validated |
| join_inner | lin | 16533.354 | 1.00× | validated |
| join_inner | sqlite | 52486.917 | 3.17× | validated |
| join_filter | duckdb | 14402.396 | 1.70× | validated |
| join_filter | postgres | 25744.750 | 3.03× | validated |
| join_filter | mysql | 91518.354 | 10.78× | validated |
| join_filter | mongo | 738358.770 | 86.98× | validated |
| join_filter | pandas | 19947.750 | 2.35× | validated |
| join_filter | lin | 8488.688 | 1.00× | validated |
| join_filter | sqlite | 26925.812 | 3.17× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
