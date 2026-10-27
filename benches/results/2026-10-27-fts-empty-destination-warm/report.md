# Known-empty FTS destination: warm DELETE diagnostic

Compared production main047 source with the correctness-tested FTS-only prototype. Six rotated baseline/control/candidate rounds, 18 fresh processes, 864 observations, 32 deletes per observation (27648 timed deletes). Sizes 1k/10k/100k; 16 batches per size. Each batch deletes 32 distinct prepared ids in a transaction, drops full handles inside the timer, then rolls back outside it. Fixtures are reused within a process; this is a warm batch diagnostic, not isolated fresh-one-row or durable qualification.

Readback: full affected row equality after rollback, collection cardinality and indexed wing count. Separate previous 2700-step FTS representation/rebuild oracle and full tests remain in ../2026-10-27-fts-empty-destination/. This timing harness does not independently validate every FTS posting.

| Rows | Median gain % | Median absolute A/A % | Positive pairs |
|---|---:|---:|---:|
| 1000 | 1.147 | 1.928 | 5/6 |
| 10000 | 4.622 | 2.815 | 5/6 |
| 100000 | 3.323 | 3.075 | 5/6 |

No qualified improvement: all sizes have only 5/6 positive pairs; 1k gain is below A/A, and 100k is near A/A. Preserve this evidence without promotion or unchanged retries seeking a green result. No SQLite or other peer speed claim. Source restored; source/binary hashes in provenance.json.

Ownership investigation: Store::durable_commit_prepared consumes an owned LogRecord/Pack, while exec_stmt clones UPDATE rows for the public handle. A possible API-preserving single-autocommit path is to recover the unchanged Pack after successful append and move its rows into Handle. Existing public durable_commit signature can remain a wrapper. Explicit transactions must still retain their packs and journal-byte accounting. Required checks: full returned rows and Done.n; output quota; rollback on encoding/I/O failure; Full reopen/replay; multi-statement final-query semantics; unchanged transaction journal quota. Not implemented or measured by this report.
