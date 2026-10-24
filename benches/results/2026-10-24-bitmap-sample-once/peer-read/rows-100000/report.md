# Validated read API benchmark

Rows: 100000; process repetitions: 3; host: macOS-26.7.2-arm64-arm-64bit-Mach-O

Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values.
Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.
Medians are batch averages, not individual latency percentiles. Each process is aggregated first.

| Case | Engine | Median µs/op | Peer / Lin | Status |
|---|---|---:|---:|---|
| point_get | lin | 0.114 | 1.00× | validated |
| point_get | sqlite | 0.967 | 8.52× | validated |
| point_get | duckdb | 39.286 | 345.92× | validated |
| point_get | postgres | 245.926 | 2165.40× | validated |
| point_get | mysql | 325.790 | 2868.60× | validated |
| point_get | mongo | 414.110 | 3646.27× | validated |
| point_get | pandas | 4.482 | 39.47× | validated |
| filter_eq | lin | 0.858 | 1.00× | validated |
| filter_eq | sqlite | 673.735 | 785.11× | validated |
| filter_eq | duckdb | 441.748 | 514.77× | validated |
| filter_eq | postgres | 2427.542 | 2828.83× | validated |
| filter_eq | mysql | 3461.542 | 4033.75× | validated |
| filter_eq | mongo | 5139.708 | 5989.33× | validated |
| filter_eq | pandas | 2304.573 | 2685.53× | validated |
| text_substr | lin | 206.543 | 1.00× | validated |
| text_substr | sqlite | 3319.375 | 16.07× | validated |
| text_substr | duckdb | 474.475 | 2.30× | validated |
| text_substr | postgres | 5344.833 | 25.88× | validated |
| text_substr | mysql | 9587.250 | 46.42× | validated |
| text_substr | mongo | 27200.125 | 131.69× | validated |
| text_substr | pandas | 4531.917 | 21.94× | validated |
| materialize | lin | 5635.896 | 1.00× | validated |
| materialize | sqlite | 19671.000 | 3.49× | validated |
| materialize | duckdb | 12508.125 | 2.22× | validated |
| materialize | postgres | 14367.271 | 2.55× | validated |
| materialize | mysql | 83219.438 | 14.77× | validated |
| materialize | mongo | 43345.521 | 7.69× | validated |
| materialize | pandas | 16891.250 | 3.00× | validated |
| join_inner | lin | 16129.583 | 1.00× | validated |
| join_inner | sqlite | 50015.666 | 3.10× | validated |
| join_inner | duckdb | 29298.917 | 1.82× | validated |
| join_inner | postgres | 46008.020 | 2.85× | validated |
| join_inner | mysql | 166638.396 | 10.33× | validated |
| join_inner | mongo | 1218022.771 | 75.51× | validated |
| join_inner | pandas | 36161.978 | 2.24× | validated |
| join_filter | lin | 7667.167 | 1.00× | validated |
| join_filter | sqlite | 24681.500 | 3.22× | validated |
| join_filter | duckdb | 13936.167 | 1.82× | validated |
| join_filter | postgres | 22369.541 | 2.92× | validated |
| join_filter | mysql | 91871.271 | 11.98× | validated |
| join_filter | mongo | 565563.083 | 73.76× | validated |
| join_filter | pandas | 18590.750 | 2.42× | validated |

Versions: {"duckdb": "1.5.6", "lin": "0.4.0", "mongo": "8.0.34", "mysql": "8.4.11", "pandas": "3.0.6", "postgres": "160015", "sqlite": "3.54.1"}

Missing/failed peers are never counted as wins.
