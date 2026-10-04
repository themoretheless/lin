# Current peer read API matrix

Source: main 588136e. Pinned current release peer_bench worker; binary SHA256 in verification.json. Three independent processes per size, eight samples per case/process, at 1000/10000/100000 documents. Six matched read APIs: point get, equality count, substring count, materialize, inner join and filtered inner join. Exact values/columns/multiplicity are validated outside timing. All 42 observations in each child contain eight samples.

| Documents | Available aggregate comparisons | Lin aggregate wins | Missing peers |
|---|---:|---:|---|
| 1000 | 36 | 36 | MSSQL, Kusto |
| 10000 | 36 | 36 | MSSQL, Kusto |
| 100000 | 36 | 36 | MSSQL, Kusto |

All 108 available aggregate comparisons favor Lin. Aggregation takes a median within each process first, then a median of three process medians; this does not say Lin wins every individual process/sample. Raw batch averages are not operation-latency percentiles. The full gate remains INCOMPLETE (exit 2 for each dataset), solely due to absent MSSQL/Kusto configuration. Missing peers are never counted as wins.

| Peer | Minimum aggregate peer/Lin ratio across all 18 cases |
|---|---:|
| sqlite | 2.86x |
| duckdb | 1.16x |
| postgres | 2.61x |
| mysql | 7.31x |
| mongo | 5.45x |
| pandas | 1.46x |

This is a comparison of the implemented API paths, not intrinsic engine CPU or universal workload superiority. Lin uses prepared Rust execution and excludes JSON IPC; Python peers include their DBAPI/driver/DataFrame operations, server round trips and result materialization. Schema, seeding/preparation are outside timing. Engine order rotates across independent processes; case order remains fixed within each engine. Host load, CPU clocks and server scheduling are uncontrolled. The closest DuckDB/pandas cases have less margin than the older 2026-10-03 matrix; no causal claim about source changes explains that shift.

Dedicated ARM64 PostgreSQL 16, MySQL 8.4 and MongoDB 8.0 Docker instances use tmpfs, unique owned linbench_ fixtures and the existing clients. Actual runtime engine/client versions are in each dataset run.json. The tmpfs server results do not prove physical disk durability. The sentinel table/database rows stayed exactly unchanged after each dataset, and no fixture objects remained. Created sentinels were dropped in finally; runner exited 0 after cleanup and all three owned containers were verified stopped. final-state.json records architecture/stopped state and source-hash revalidation; verification.json records per-dataset sentinel and fixture checks.

No builds/tests overlapped timing. No production source changed. Build log, pinned binary/source hashes, exact runner and summarizer, raw process outputs and runtime versions are retained here.

The full nine-engine objective is still incomplete. Current native 1k evidence from the immediately preceding insertion comparison uses this same production baseline: Lin 1.1601665 ms vs SQLite 1.11507325 ms, 0/6 same-process wins. Durable 1k still loses in the retained sparse-classifier evidence. Local durable 10k has a small confirmed lead over SQLite, while full write comparisons across all peers and MSSQL/Kusto measurements remain unproven. Read results do not erase these gaps or redefine completion around reads alone.
