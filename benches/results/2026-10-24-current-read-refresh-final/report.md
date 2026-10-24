# Current read matrix: 108/108 aggregate wins

Source `f127a3fbebc0c9b1843bfb19b93fd5fc9ba2a41e`. Frozen worker built on identical core/benchmark sources during the preceding native refresh; its binary hash and every core source hash are verified before and after this campaign. No production changes for this run.

## Method

Six scenarios: point_get, filter_eq, text_substr, materialize, join_inner, join_filter; 1k/10k/100k datasets; Lin versus SQLite, DuckDB, PostgreSQL, MySQL, MongoDB and pandas. Three independent processes per size, rotated engine order, 12 measured samples per scenario/engine/process. Every result is validated for matching columns/values/multiplicity. Input fixtures, setup, validation and Lin JSON IPC are outside timing. Lin uses Rust prepared API; peers use Python DBAPI/driver/DataFrame and server timings include transfer/round trips. No equal-API or physical-durability claim.

| Rows | Case | Lin µs | Closest peer | Peer µs | Peer/Lin |
|---:|---|---:|---|---:|---:|
| 1000 | point_get | 0.113 | sqlite | 0.902 | 7.963 |
| 1000 | filter_eq | 0.634 | sqlite | 7.425 | 11.710 |
| 1000 | text_substr | 1.640 | sqlite | 32.361 | 19.728 |
| 1000 | materialize | 39.411 | sqlite | 162.030 | 4.111 |
| 1000 | join_inner | 130.918 | duckdb | 352.748 | 2.694 |
| 1000 | join_filter | 67.320 | sqlite | 198.795 | 2.953 |
| 10000 | point_get | 0.121 | sqlite | 0.945 | 7.794 |
| 10000 | filter_eq | 0.650 | sqlite | 67.707 | 104.116 |
| 10000 | text_substr | 16.104 | duckdb | 97.168 | 6.034 |
| 10000 | materialize | 361.420 | duckdb | 1266.945 | 3.505 |
| 10000 | join_inner | 1264.194 | duckdb | 2674.084 | 2.115 |
| 10000 | join_filter | 628.676 | duckdb | 1399.833 | 2.227 |
| 100000 | point_get | 0.112 | sqlite | 0.978 | 8.732 |
| 100000 | filter_eq | 0.845 | duckdb | 420.371 | 497.757 |
| 100000 | text_substr | 207.469 | duckdb | 469.856 | 2.265 |
| 100000 | materialize | 4988.105 | duckdb | 12343.083 | 2.475 |
| 100000 | join_inner | 15733.062 | duckdb | 28571.229 | 1.816 |
| 100000 | join_filter | 7060.708 | duckdb | 14283.896 | 2.023 |

## Verification and limits

All 378 process/case/engine observations are complete (4536 measured samples). All three per-size --require-wins invocations exited 0. Sentinel tables/databases retained their exact expected values after every size; no benchmark fixtures remained. All three task-owned servers stopped. Versions, native ARM64 image digests, tmpfs settings and unchanged sync settings are in environment.json. Raw process reports and samples are preserved; unrelated host activity is uncontrolled. These are medians of per-process medians, not p99 and not a paired before/after optimization estimate.

The matching current native insertion campaign is [15/15 against five peers](../2026-10-24-current-native-refresh/report.md). The dictionary writer candidate was [rejected](../2026-10-24-dictionary-wal-writer/report.md), leaving automatic codec-5 writing disabled.

## Objective remains open

MSSQL and Kusto were not configured and are not counted. Durable 1k still loses to SQLite. This six-case read matrix cannot establish superiority on every workload or replace the missing-peer and write/durability contracts. Pandas is covered here only for reads; DataFrame construction is a separate contract from transactional insertion.
