# Full local read matrix after prepared COUNT correction

Date: 2026-10-24. Production source `3a2e8d558abf70f9c91c9f3c1020276b1ed470cc`. **54/54 aggregate comparisons won** across six scenarios, three sizes and three peers. All nine independent process runs passed exact result validation and --require-wins. This supersedes the pre-fix local 43/54 result for these same scenarios, not historical server measurements.

Pinned binary reused from the accepted prepared-count experiment; SHA256 and source hashes in provenance.json, verified unchanged before and after measurement. No rebuild occurred during measurement.

## Scope and method

SQLite, DuckDB and pandas; 1k/10k/100k rows; point_get, filter_eq, text_substr, materialize, join_inner and join_filter. Three independent Python processes per size with rotated engine order, 12 samples per case, target 5ms. Median of process sample medians. Exact columns/values/multiplicity checked before/after timing; fixtures/preparation/validation/IPC/teardown excluded. Row order is not part of these query contracts. Raw samples, client/runtime versions and commands retained in rows-* directories and run.py.

Lin prepared Rust row API versus Python DBAPI/DataFrame interfaces; these are warm API comparisons including peer dispatch/materialization, not intrinsic engine CPU or individual-operation p99. No builds or unrelated benchmark workloads initiated by this runner overlap the matrix; other host activity was not controlled. See docs/benchmark-contract.md. The preceding two-series six-pair count experiment establishes repeatability of the changed paths; this full matrix checks all six scenarios, without claiming statistical universality.

PostgreSQL, MySQL and MongoDB are **not rerun** here: prior dedicated benchmark containers are absent. MSSQL/Kusto endpoint configuration is absent (presence checked without exposing secrets). Missing peers are not wins. Physical-disk durability and native ingestion are separate contracts; durable SQLite insertion gaps remain unresolved.

## Results

| Rows | Case | Peer | Lin µs | Peer µs | Peer/Lin |
|---:|---|---|---:|---:|---:|
| 1000 | point_get | sqlite | 0.113 | 0.893 | 7.886 |
| 1000 | point_get | duckdb | 0.113 | 39.905 | 352.337 |
| 1000 | point_get | pandas | 0.113 | 4.419 | 39.016 |
| 1000 | filter_eq | sqlite | 0.632 | 7.381 | 11.684 |
| 1000 | filter_eq | duckdb | 0.632 | 130.319 | 206.305 |
| 1000 | filter_eq | pandas | 0.632 | 55.775 | 88.296 |
| 1000 | text_substr | sqlite | 1.784 | 31.051 | 17.402 |
| 1000 | text_substr | duckdb | 1.784 | 57.717 | 32.346 |
| 1000 | text_substr | pandas | 1.784 | 73.344 | 41.103 |
| 1000 | materialize | sqlite | 36.849 | 164.253 | 4.457 |
| 1000 | materialize | duckdb | 36.849 | 295.914 | 8.030 |
| 1000 | materialize | pandas | 36.849 | 364.792 | 9.900 |
| 1000 | join_inner | sqlite | 134.142 | 389.283 | 2.902 |
| 1000 | join_inner | duckdb | 134.142 | 345.870 | 2.578 |
| 1000 | join_inner | pandas | 134.142 | 702.681 | 5.238 |
| 1000 | join_filter | sqlite | 69.933 | 202.139 | 2.890 |
| 1000 | join_filter | duckdb | 69.933 | 241.596 | 3.455 |
| 1000 | join_filter | pandas | 69.933 | 615.888 | 8.807 |
| 10000 | point_get | sqlite | 0.115 | 0.893 | 7.744 |
| 10000 | point_get | duckdb | 0.115 | 40.129 | 347.886 |
| 10000 | point_get | pandas | 0.115 | 4.392 | 38.072 |
| 10000 | filter_eq | sqlite | 0.664 | 67.678 | 101.864 |
| 10000 | filter_eq | duckdb | 0.664 | 185.892 | 279.789 |
| 10000 | filter_eq | pandas | 0.664 | 264.250 | 397.727 |
| 10000 | text_substr | sqlite | 16.654 | 326.297 | 19.592 |
| 10000 | text_substr | duckdb | 16.654 | 103.488 | 6.214 |
| 10000 | text_substr | pandas | 16.654 | 480.370 | 28.844 |
| 10000 | materialize | sqlite | 396.226 | 1907.646 | 4.815 |
| 10000 | materialize | duckdb | 396.226 | 1287.445 | 3.249 |
| 10000 | materialize | pandas | 396.226 | 1816.667 | 4.585 |
| 10000 | join_inner | sqlite | 1364.403 | 4524.458 | 3.316 |
| 10000 | join_inner | duckdb | 1364.403 | 2617.459 | 1.918 |
| 10000 | join_inner | pandas | 1364.403 | 3846.479 | 2.819 |
| 10000 | join_filter | sqlite | 691.426 | 2334.146 | 3.376 |
| 10000 | join_filter | duckdb | 691.426 | 1440.708 | 2.084 |
| 10000 | join_filter | pandas | 691.426 | 2208.135 | 3.194 |
| 100000 | point_get | sqlite | 0.116 | 0.982 | 8.497 |
| 100000 | point_get | duckdb | 0.116 | 39.810 | 344.579 |
| 100000 | point_get | pandas | 0.116 | 4.395 | 38.041 |
| 100000 | filter_eq | sqlite | 0.844 | 706.926 | 837.726 |
| 100000 | filter_eq | duckdb | 0.844 | 439.148 | 520.402 |
| 100000 | filter_eq | pandas | 0.844 | 2194.708 | 2600.788 |
| 100000 | text_substr | sqlite | 206.620 | 3444.562 | 16.671 |
| 100000 | text_substr | duckdb | 206.620 | 475.194 | 2.300 |
| 100000 | text_substr | pandas | 206.620 | 4473.292 | 21.650 |
| 100000 | materialize | sqlite | 4705.646 | 21204.667 | 4.506 |
| 100000 | materialize | duckdb | 4705.646 | 13032.021 | 2.769 |
| 100000 | materialize | pandas | 4705.646 | 18369.958 | 3.904 |
| 100000 | join_inner | sqlite | 15773.334 | 54282.375 | 3.441 |
| 100000 | join_inner | duckdb | 15773.334 | 29638.896 | 1.879 |
| 100000 | join_inner | pandas | 15773.334 | 35529.395 | 2.252 |
| 100000 | join_filter | sqlite | 7951.562 | 26839.688 | 3.375 |
| 100000 | join_filter | duckdb | 7951.562 | 14510.334 | 1.825 |
| 100000 | join_filter | pandas | 7951.562 | 19033.708 | 2.394 |

Smallest aggregate margin: 1.825× at 100000 rows, join_filter versus duckdb. Values are workload-specific and must not be generalized to arbitrary queries/data.

## Remaining qualification

Recreate dedicated owned PostgreSQL/MySQL/MongoDB instances with explicit versions/settings, verify sentinels and fixture cleanup, rerun full read and native ingestion contracts. MSSQL/Kusto need dedicated configured endpoints. Continue durable insertion optimization without weakening sync or feature work.
