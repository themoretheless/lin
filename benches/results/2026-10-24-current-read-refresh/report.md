# Current local read API matrix — 2026-10-24

Source `28af2dc0107e82cecfc49fd7ee3ed6a27d988b2c`; source hashes verified unchanged after measurement. Pinned release worker hash in binary-sha256.json. **43/54 aggregate comparisons won** against SQLite, DuckDB and pandas at 1k/10k/100k. All exact result validations passed; every run is complete. Exit 3 from --require-wins correctly records measured losses, not execution failures.

Three independent Python processes per size, engine order rotated, 12 samples, target sample 5ms, six existing read scenarios. Reported figures are medians of process sample medians, not individual-operation p99. Fixture creation, JSON IPC and exact validation excluded. Lin prepared Rust API versus Python driver/DataFrame APIs; unequal dispatch/materialization costs remain. No builds initiated by this runner overlap measurement; other host activity was not controlled. See docs/benchmark-contract.md for full scope. Versions and raw samples in each run.json.

PostgreSQL/MySQL/MongoDB were not rerun: no dedicated benchmark containers were present. MSSQL/Kusto endpoints remain unqualified. Their historical results cannot establish current wins. Native/durable writes are separate contracts.

| Rows | Case | Peer | Lin µs | Peer µs | Peer/Lin | Result |
|---:|---|---|---:|---:|---:|---|
| 1000 | point_get | sqlite | 0.174 | 0.934 | 5.366 | win |
| 1000 | point_get | duckdb | 0.174 | 41.990 | 241.235 | win |
| 1000 | point_get | pandas | 0.174 | 4.581 | 26.315 | win |
| 1000 | filter_eq | sqlite | 58.269 | 7.569 | 0.130 | loss |
| 1000 | filter_eq | duckdb | 58.269 | 147.864 | 2.538 | win |
| 1000 | filter_eq | pandas | 58.269 | 161.707 | 2.775 | win |
| 1000 | text_substr | sqlite | 43.772 | 32.073 | 0.733 | loss |
| 1000 | text_substr | duckdb | 43.772 | 74.813 | 1.709 | win |
| 1000 | text_substr | pandas | 43.772 | 98.630 | 2.253 | win |
| 1000 | materialize | sqlite | 39.214 | 165.371 | 4.217 | win |
| 1000 | materialize | duckdb | 39.214 | 303.207 | 7.732 | win |
| 1000 | materialize | pandas | 39.214 | 475.229 | 12.119 | win |
| 1000 | join_inner | sqlite | 133.981 | 393.766 | 2.939 | win |
| 1000 | join_inner | duckdb | 133.981 | 406.951 | 3.037 | win |
| 1000 | join_inner | pandas | 133.981 | 1504.042 | 11.226 | win |
| 1000 | join_filter | sqlite | 67.269 | 203.813 | 3.030 | win |
| 1000 | join_filter | duckdb | 67.269 | 268.233 | 3.987 | win |
| 1000 | join_filter | pandas | 67.269 | 6962.230 | 103.498 | win |
| 10000 | point_get | sqlite | 0.116 | 0.969 | 8.363 | win |
| 10000 | point_get | duckdb | 0.116 | 37.406 | 322.726 | win |
| 10000 | point_get | pandas | 0.116 | 4.497 | 38.800 | win |
| 10000 | filter_eq | sqlite | 260.064 | 69.951 | 0.269 | loss |
| 10000 | filter_eq | duckdb | 260.064 | 190.983 | 0.734 | loss |
| 10000 | filter_eq | pandas | 260.064 | 262.930 | 1.011 | win |
| 10000 | text_substr | sqlite | 294.321 | 327.424 | 1.112 | win |
| 10000 | text_substr | duckdb | 294.321 | 103.197 | 0.351 | loss |
| 10000 | text_substr | pandas | 294.321 | 484.745 | 1.647 | win |
| 10000 | materialize | sqlite | 369.803 | 1891.344 | 5.114 | win |
| 10000 | materialize | duckdb | 369.803 | 1259.411 | 3.406 | win |
| 10000 | materialize | pandas | 369.803 | 1854.063 | 5.014 | win |
| 10000 | join_inner | sqlite | 1347.903 | 4518.770 | 3.352 | win |
| 10000 | join_inner | duckdb | 1347.903 | 2675.938 | 1.985 | win |
| 10000 | join_inner | pandas | 1347.903 | 4028.667 | 2.989 | win |
| 10000 | join_filter | sqlite | 657.646 | 2337.260 | 3.554 | win |
| 10000 | join_filter | duckdb | 657.646 | 1404.660 | 2.136 | win |
| 10000 | join_filter | pandas | 657.646 | 2332.468 | 3.547 | win |
| 100000 | point_get | sqlite | 0.116 | 0.999 | 8.609 | win |
| 100000 | point_get | duckdb | 0.116 | 40.585 | 349.630 | win |
| 100000 | point_get | pandas | 0.116 | 4.484 | 38.631 | win |
| 100000 | filter_eq | sqlite | 5940.604 | 692.587 | 0.117 | loss |
| 100000 | filter_eq | duckdb | 5940.604 | 446.327 | 0.075 | loss |
| 100000 | filter_eq | pandas | 5940.604 | 2298.323 | 0.387 | loss |
| 100000 | text_substr | sqlite | 6087.333 | 3359.979 | 0.552 | loss |
| 100000 | text_substr | duckdb | 6087.333 | 487.109 | 0.080 | loss |
| 100000 | text_substr | pandas | 6087.333 | 4743.271 | 0.779 | loss |
| 100000 | materialize | sqlite | 5685.395 | 20257.270 | 3.563 | win |
| 100000 | materialize | duckdb | 5685.395 | 12552.916 | 2.208 | win |
| 100000 | materialize | pandas | 5685.395 | 18344.688 | 3.227 | win |
| 100000 | join_inner | sqlite | 16672.855 | 51515.145 | 3.090 | win |
| 100000 | join_inner | duckdb | 16672.855 | 28719.583 | 1.723 | win |
| 100000 | join_inner | pandas | 16672.855 | 39103.166 | 2.345 | win |
| 100000 | join_filter | sqlite | 7858.438 | 26301.291 | 3.347 | win |
| 100000 | join_filter | duckdb | 7858.438 | 14500.833 | 1.845 | win |
| 100000 | join_filter | pandas | 7858.438 | 20026.042 | 2.548 | win |

## Next performance hypothesis

Current prepared aggregation dispatch calls execute_agg_plan before exec_query_lim. Its generic filtered COUNT fallback scans row maps, bypassing try_filter_count's index-covered count and packed-title substring count. This source observation explains a plausible cause of count losses; it is not yet a measured fix. Reuse those existing semantic paths for prepared COUNT, qualify catalog filters/grouping/mutation/now and compare pinned before/after workers. Keep all failed cases in the matrix.
