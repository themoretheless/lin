# Current peer read API matrix

Source commit: e71a17b61dfde8f6b62dd977ba7d0ccc589826f9. Pinned release peer_bench worker.
Three independent processes per size, eight samples per case/process. Six matched
read APIs: point get, equality count, substring count, materialize, inner join,
filtered inner join. Exact result values/multiplicity validated outside timing.
Schema/seeding/preparation and Lin JSON IPC excluded. Python peer timings include
DBAPI/driver/DataFrame work and server round-trip/materialization. This measures
these API paths; it is not an intrinsic engine CPU or universal workload claim.

| Documents | Validated available peer comparisons | Lin aggregate wins | Missing peers |
|---|---:|---:|---|
| 1,000 | 36 | 36 | MSSQL, Kusto |
| 10,000 | 36 | 36 | MSSQL, Kusto |
| 100,000 | 36 | 36 | MSSQL, Kusto |

All 108 available comparisons favor Lin by medians aggregated within each
process first, then across the three processes. Raw batch averages are not
individual latency percentiles. The full gate is INCOMPLETE (exit 2) at every
size because MSSQL/Kusto endpoints are absent; unavailable cases are never wins.

| Peer | Minimum aggregate peer / Lin ratio across 18 read cases |
|---|---:|
| sqlite | 2.93x |
| duckdb | 1.97x |
| postgres | 2.73x |
| mysql | 12.31x |
| mongo | 8.09x |
| pandas | 2.68x |

Dedicated native ARM Docker PostgreSQL/MySQL/MongoDB instances on tmpfs; schema
objects use unique owned names. Sentinel tables/database exact rows remained
unchanged after each dataset. No linbench_ fixture objects remained. All three
owned containers stopped in finally. verification.json records cleanup/sentinels
and binary SHA256; per-dataset reports include actual engine versions.

No builds/tests overlapped timing. Case/engine order rotates across processes;
host load, CPU clocks and server scheduling are uncontrolled. Source/script hashes
and raw reports retained. This read matrix does not erase SQLite small/native
and durable insert losses from the existing write evidence. Native ingestion,
physical disk durability, and MSSQL/Kusto are not verified by this run. The full
eight-engine goal remains unproven.
