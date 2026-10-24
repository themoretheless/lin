# Current six-peer full read API matrix — 2026-10-24

Production source `4b0fb41362927e2de2159959bc334f9d54cba9e4` (evidence-only successor of accepted prepared COUNT change `3a2e8d5`). **108/108 aggregate comparisons won** across six query scenarios, three dataset sizes and six peers. All nine independent process runs passed exact validation; all three size runs passed --require-wins. This requalifies the current source for these read contracts; it is not a universal database-speed claim.

## Provenance and methodology

Pinned `/tmp/lin-prepared-count-candidate` binary reused after SHA256 and all src Rust hashes matched accepted source. Source hashes checked unchanged before/after measurement. Hashes, image IDs/digests/architecture, server settings and final states retained in environment.json. No builds or unrelated benchmark campaigns initiated by this runner overlap measurements; other host activity was not controlled.

1k/10k/100k deterministic fixtures; point_get, filter_eq, text_substr, materialize, join_inner and join_filter. Three independent processes/connections per size, rotated engine order, 12 samples, target 5ms. Median of process sample medians. Exact columns/values/multiplicities validated outside timers before/after; row order is not part of these contracts. Setup/preparation/validation/JSON IPC/teardown excluded. Raw samples/versions in rows-*/run.json and each process directory; runnable commands in measure.py.

Lin uses prepared Rust row API; SQLite/DuckDB/PostgreSQL/MySQL/MongoDB/pandas use Python driver/DataFrame APIs. Peer times include relevant dispatch, network and materialization costs. These measurements are neither isolated engine CPU nor individual-operation p99. See docs/benchmark-contract.md. Fixed within-process case order and shared-host noise limit broader conclusions.

## Servers and cleanup

Fresh task-owned PostgreSQL 16-alpine, MySQL 8.4 and MongoDB 8.0 containers, bound only to 127.0.0.1, all native ARM64. Images pinned by resolved image ID for launch. Data directories tmpfs; not physical-disk durability evidence. PostgreSQL synchronous_commit/fsync/full_page_writes on; MySQL innodb_flush_log_at_trx_commit=1 and sync_binlog=1, unchanged defaults. Runtime versions per dataset retained in overview.json. Lin memory database and default feature work remain unchanged.

Independent sentinels verified unchanged after every dataset; no linbench_ fixture tables/databases remained. Sentinel objects dropped by their creator. All three owned containers stopped, retained for subsequent benchmark use; other containers untouched. verification.json and environment.json record the checks. The runner verified no local peer URL overrides were present; adapters used the dedicated default localhost endpoints.

## Results

| Rows | Case | Peer | Lin µs | Peer µs | Peer/Lin | Result |
|---:|---|---|---:|---:|---:|---|
| 1000 | point_get | sqlite | 0.120 | 0.941 | 7.817 | win |
| 1000 | point_get | duckdb | 0.120 | 39.972 | 332.052 | win |
| 1000 | point_get | postgres | 0.120 | 322.779 | 2681.351 | win |
| 1000 | point_get | mysql | 0.120 | 347.277 | 2884.863 | win |
| 1000 | point_get | mongo | 0.120 | 470.891 | 3911.734 | win |
| 1000 | point_get | pandas | 0.120 | 4.695 | 38.999 | win |
| 1000 | filter_eq | sqlite | 0.682 | 7.525 | 11.027 | win |
| 1000 | filter_eq | duckdb | 0.682 | 136.216 | 199.606 | win |
| 1000 | filter_eq | postgres | 0.682 | 340.510 | 498.971 | win |
| 1000 | filter_eq | mysql | 0.682 | 364.205 | 533.693 | win |
| 1000 | filter_eq | mongo | 0.682 | 542.185 | 794.497 | win |
| 1000 | filter_eq | pandas | 0.682 | 56.342 | 82.562 | win |
| 1000 | text_substr | sqlite | 1.927 | 32.283 | 16.756 | win |
| 1000 | text_substr | duckdb | 1.927 | 59.179 | 30.717 | win |
| 1000 | text_substr | postgres | 1.927 | 363.364 | 188.605 | win |
| 1000 | text_substr | mysql | 1.927 | 416.361 | 216.113 | win |
| 1000 | text_substr | mongo | 1.927 | 822.094 | 426.710 | win |
| 1000 | text_substr | pandas | 1.927 | 76.567 | 39.742 | win |
| 1000 | materialize | sqlite | 37.780 | 168.690 | 4.465 | win |
| 1000 | materialize | duckdb | 37.780 | 292.602 | 7.745 | win |
| 1000 | materialize | postgres | 37.780 | 455.664 | 12.061 | win |
| 1000 | materialize | mysql | 37.780 | 1218.844 | 32.261 | win |
| 1000 | materialize | mongo | 37.780 | 1383.875 | 36.629 | win |
| 1000 | materialize | pandas | 37.780 | 379.943 | 10.057 | win |
| 1000 | join_inner | sqlite | 130.820 | 387.484 | 2.962 | win |
| 1000 | join_inner | duckdb | 130.820 | 362.021 | 2.767 | win |
| 1000 | join_inner | postgres | 130.820 | 685.191 | 5.238 | win |
| 1000 | join_inner | mysql | 130.820 | 2360.052 | 18.040 | win |
| 1000 | join_inner | mongo | 130.820 | 12630.250 | 96.547 | win |
| 1000 | join_inner | pandas | 130.820 | 720.281 | 5.506 | win |
| 1000 | join_filter | sqlite | 64.633 | 201.593 | 3.119 | win |
| 1000 | join_filter | duckdb | 64.633 | 245.580 | 3.800 | win |
| 1000 | join_filter | postgres | 64.633 | 507.782 | 7.856 | win |
| 1000 | join_filter | mysql | 64.633 | 1444.271 | 22.346 | win |
| 1000 | join_filter | mongo | 64.633 | 6663.896 | 103.103 | win |
| 1000 | join_filter | pandas | 64.633 | 627.080 | 9.702 | win |
| 10000 | point_get | sqlite | 0.122 | 0.952 | 7.802 | win |
| 10000 | point_get | duckdb | 0.122 | 39.640 | 324.747 | win |
| 10000 | point_get | postgres | 0.122 | 280.714 | 2299.715 | win |
| 10000 | point_get | mysql | 0.122 | 348.835 | 2857.792 | win |
| 10000 | point_get | mongo | 0.122 | 455.505 | 3731.674 | win |
| 10000 | point_get | pandas | 0.122 | 4.522 | 37.045 | win |
| 10000 | filter_eq | sqlite | 0.697 | 68.644 | 98.467 | win |
| 10000 | filter_eq | duckdb | 0.697 | 194.188 | 278.557 | win |
| 10000 | filter_eq | postgres | 0.697 | 488.081 | 700.137 | win |
| 10000 | filter_eq | mysql | 0.697 | 690.905 | 991.082 | win |
| 10000 | filter_eq | mongo | 0.697 | 982.203 | 1408.940 | win |
| 10000 | filter_eq | pandas | 0.697 | 265.651 | 381.068 | win |
| 10000 | text_substr | sqlite | 16.252 | 328.971 | 20.242 | win |
| 10000 | text_substr | duckdb | 16.252 | 105.150 | 6.470 | win |
| 10000 | text_substr | postgres | 16.252 | 776.854 | 47.800 | win |
| 10000 | text_substr | mysql | 16.252 | 1306.820 | 80.409 | win |
| 10000 | text_substr | mongo | 16.252 | 3248.166 | 199.862 | win |
| 10000 | text_substr | pandas | 16.252 | 499.544 | 30.737 | win |
| 10000 | materialize | sqlite | 382.319 | 1858.802 | 4.862 | win |
| 10000 | materialize | duckdb | 382.319 | 1262.844 | 3.303 | win |
| 10000 | materialize | postgres | 382.319 | 1911.833 | 5.001 | win |
| 10000 | materialize | mysql | 382.319 | 9827.209 | 25.704 | win |
| 10000 | materialize | mongo | 382.319 | 5409.708 | 14.150 | win |
| 10000 | materialize | pandas | 382.319 | 1908.114 | 4.991 | win |
| 10000 | join_inner | sqlite | 1349.042 | 4522.895 | 3.353 | win |
| 10000 | join_inner | duckdb | 1349.042 | 2627.188 | 1.947 | win |
| 10000 | join_inner | postgres | 1349.042 | 4628.833 | 3.431 | win |
| 10000 | join_inner | mysql | 1349.042 | 20772.104 | 15.398 | win |
| 10000 | join_inner | mongo | 1349.042 | 115275.583 | 85.450 | win |
| 10000 | join_inner | pandas | 1349.042 | 4022.854 | 2.982 | win |
| 10000 | join_filter | sqlite | 678.141 | 2327.250 | 3.432 | win |
| 10000 | join_filter | duckdb | 678.141 | 1395.229 | 2.057 | win |
| 10000 | join_filter | postgres | 678.141 | 2337.709 | 3.447 | win |
| 10000 | join_filter | mysql | 678.141 | 11034.354 | 16.271 | win |
| 10000 | join_filter | mongo | 678.141 | 59633.500 | 87.937 | win |
| 10000 | join_filter | pandas | 678.141 | 2397.792 | 3.536 | win |
| 100000 | point_get | sqlite | 0.114 | 0.994 | 8.715 | win |
| 100000 | point_get | duckdb | 0.114 | 39.822 | 349.149 | win |
| 100000 | point_get | postgres | 0.114 | 249.988 | 2191.834 | win |
| 100000 | point_get | mysql | 0.114 | 327.338 | 2870.024 | win |
| 100000 | point_get | mongo | 0.114 | 403.242 | 3535.535 | win |
| 100000 | point_get | pandas | 0.114 | 4.468 | 39.178 | win |
| 100000 | filter_eq | sqlite | 0.847 | 687.610 | 811.911 | win |
| 100000 | filter_eq | duckdb | 0.847 | 419.087 | 494.846 | win |
| 100000 | filter_eq | postgres | 0.847 | 2284.177 | 2697.092 | win |
| 100000 | filter_eq | mysql | 0.847 | 3318.438 | 3918.318 | win |
| 100000 | filter_eq | mongo | 0.847 | 4957.833 | 5854.070 | win |
| 100000 | filter_eq | pandas | 0.847 | 2295.615 | 2710.597 | win |
| 100000 | text_substr | sqlite | 210.702 | 3484.958 | 16.540 | win |
| 100000 | text_substr | duckdb | 210.702 | 453.925 | 2.154 | win |
| 100000 | text_substr | postgres | 210.702 | 5066.938 | 24.048 | win |
| 100000 | text_substr | mysql | 210.702 | 9433.416 | 44.771 | win |
| 100000 | text_substr | mongo | 210.702 | 26317.730 | 124.905 | win |
| 100000 | text_substr | pandas | 210.702 | 4618.042 | 21.917 | win |
| 100000 | materialize | sqlite | 5860.438 | 20569.291 | 3.510 | win |
| 100000 | materialize | duckdb | 5860.438 | 12207.959 | 2.083 | win |
| 100000 | materialize | postgres | 5860.438 | 14636.416 | 2.497 | win |
| 100000 | materialize | mysql | 5860.438 | 83190.688 | 14.195 | win |
| 100000 | materialize | mongo | 5860.438 | 41798.770 | 7.132 | win |
| 100000 | materialize | pandas | 5860.438 | 18245.521 | 3.113 | win |
| 100000 | join_inner | sqlite | 16286.000 | 52062.395 | 3.197 | win |
| 100000 | join_inner | duckdb | 16286.000 | 28373.980 | 1.742 | win |
| 100000 | join_inner | postgres | 16286.000 | 39464.021 | 2.423 | win |
| 100000 | join_inner | mysql | 16286.000 | 166505.438 | 10.224 | win |
| 100000 | join_inner | mongo | 16286.000 | 1120137.604 | 68.779 | win |
| 100000 | join_inner | pandas | 16286.000 | 37419.104 | 2.298 | win |
| 100000 | join_filter | sqlite | 7897.229 | 26087.625 | 3.303 | win |
| 100000 | join_filter | duckdb | 7897.229 | 13436.791 | 1.701 | win |
| 100000 | join_filter | postgres | 7897.229 | 21037.354 | 2.664 | win |
| 100000 | join_filter | mysql | 7897.229 | 90613.250 | 11.474 | win |
| 100000 | join_filter | mongo | 7897.229 | 577422.833 | 73.117 | win |
| 100000 | join_filter | pandas | 7897.229 | 19349.188 | 2.450 | win |

Minimum aggregate margin per peer:

| Peer | Minimum peer/Lin | Rows | Case |
|---|---:|---:|---|
| sqlite | 2.962 | 1000 | join_inner |
| duckdb | 1.701 | 100000 | join_filter |
| postgres | 2.423 | 100000 | join_inner |
| mysql | 10.224 | 100000 | join_inner |
| mongo | 7.132 | 100000 | materialize |
| pandas | 2.298 | 100000 | join_inner |

## Remaining goal

MSSQL and Kusto endpoint configuration remains absent; no victory recorded for them. This read matrix does not establish native ingestion or durable insertion wins. Current durable SQLite insertion gaps remain. Next refresh native bulk insertion against owned PostgreSQL/MySQL/MongoDB plus local SQLite/DuckDB with exact fixtures; preserve unsupported pandas/MSSQL/Kusto ingestion distinctions. Keep the complete named-peer objective open until all required contracts have evidence.
